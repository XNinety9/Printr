//! Serveur HTTP : reçoit des tickets JSON (scripts, raccourcis iOS…) et les imprime.
//!
//! - `GET  /`      : vérification que le serveur tourne.
//! - `POST /print` : un ticket complet (même format que `printr print`).
//! - `POST /todo`  : une liste de choses à faire, `{"title"?, "items"?, "text"?}`.
//!
//! Les `POST` exigent `Authorization: Bearer <jeton>`. Avec `?preview`, la réponse est
//! l'aperçu texte du ticket, sans rien imprimer.

use std::io::Read;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::json;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::blocks::{Block, Ctx, Silent, Ticket};
use crate::output::Target;
use crate::ui;

/// Taille maximale d'un ticket reçu (les images passent par `url`, pas dans le corps).
const MAX_BODY: u64 = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TodoRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    items: Vec<String>,
    /// Une chose à faire par ligne : pratique depuis un raccourci iOS.
    #[serde(default)]
    text: Option<String>,
}

impl TodoRequest {
    fn into_ticket(self) -> Result<Ticket> {
        let mut items = self.items;
        if let Some(text) = self.text {
            items.extend(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned));
        }
        if items.is_empty() {
            bail!("aucune chose à faire (`items` ou `text`)");
        }
        Ok(Ticket {
            cut: true,
            spacing: 1,
            blocks: vec![Block::Date {}, Block::Todo { title: self.title, items }],
        })
    }
}

struct Reply {
    status: u16,
    content_type: &'static str,
    body: String,
    /// Résumé pour le journal console.
    note: String,
}

impl Reply {
    fn json(status: u16, value: serde_json::Value) -> Self {
        Self { status, content_type: "application/json; charset=utf-8", body: value.to_string(), note: String::new() }
    }
    fn error(status: u16, message: impl Into<String>) -> Self {
        let message = message.into();
        let mut reply = Self::json(status, json!({ "ok": false, "error": message }));
        reply.note = message;
        reply
    }
    fn note(mut self, note: String) -> Self {
        self.note = note;
        self
    }
}

/// Comparaison en temps constant, pour ne pas révéler le jeton par le temps de réponse.
fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn read_body(request: &mut Request) -> Result<String, Reply> {
    let mut body = String::new();
    request
        .as_reader()
        .take(MAX_BODY + 1)
        .read_to_string(&mut body)
        .map_err(|e| Reply::error(400, format!("corps illisible : {e}")))?;
    if body.len() as u64 > MAX_BODY {
        return Err(Reply::error(413, "ticket trop volumineux"));
    }
    Ok(body)
}

fn handle(request: &mut Request, token: &str, target: &Target) -> Reply {
    let url = request.url().to_owned();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let preview = query.split('&').any(|p| p == "preview" || p.starts_with("preview="));

    match (request.method(), path) {
        (Method::Get, "/") => return Reply::json(200, json!({ "ok": true, "service": "printr" })),
        (Method::Post, "/print" | "/todo") => {}
        (_, "/" | "/print" | "/todo") => return Reply::error(405, "méthode non autorisée"),
        _ => return Reply::error(404, "route inconnue"),
    }

    let authorized = header(request, "Authorization")
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|t| same_token(t.trim(), token));
    if !authorized {
        return Reply::error(401, "jeton absent ou invalide");
    }

    let body = match read_body(request) {
        Ok(body) => body,
        Err(reply) => return reply,
    };
    let ticket: Result<Ticket> = if path == "/print" {
        serde_json::from_str(&body).context("ticket JSON invalide")
    } else {
        serde_json::from_str::<TodoRequest>(&body)
            .context("liste JSON invalide")
            .and_then(TodoRequest::into_ticket)
    };
    let ticket = match ticket {
        Ok(ticket) => ticket,
        Err(e) => return Reply::error(400, format!("{e:#}")),
    };

    let (doc, reports) = ticket.build(&Ctx::new(false), &mut Silent);
    let errors: Vec<String> =
        reports.iter().filter_map(|r| r.error.as_ref().map(|e| format!("{} : {e}", r.label))).collect();
    let mut note = format!("{} bloc{}", reports.len(), if reports.len() > 1 { "s" } else { "" });
    if !errors.is_empty() {
        note.push_str(&format!(" · {} en échec ({})", errors.len(), errors.join(" ; ")));
    }
    if preview {
        let body = doc.preview(ticket.cut);
        return Reply { status: 200, content_type: "text/plain; charset=utf-8", body, note: format!("aperçu · {note}") };
    }
    match target.send(&doc, ticket.cut) {
        Ok(()) => Reply::json(200, json!({ "ok": true, "errors": errors })).note(note),
        Err(e) => Reply::error(503, format!("{e:#}")),
    }
}

/// Sert les requêtes une par une : les impressions ne se chevauchent jamais.
pub fn serve(listen: &str, token: &str, target: &Target) -> Result<()> {
    let server = Server::http(listen).map_err(|e| anyhow::anyhow!("impossible d'écouter sur {listen} : {e}"))?;
    let ctx = Ctx::new(false);
    let cache = ctx.cache.as_ref().map(|c| c.dir().display().to_string());
    ui::banner(listen, &target.to_string(), ctx.claude.as_ref().map(|c| c.model()), cache.as_deref());
    for mut request in server.incoming_requests() {
        let start = Instant::now();
        let reply = handle(&mut request, token, target);
        let from = request.remote_addr().map_or("?".to_owned(), |a| a.ip().to_string());
        let method = request.method().to_string();
        let path = request.url().split('?').next().unwrap_or("").to_owned();
        ui::request(&method, &path, reply.status, &from, start.elapsed(), &reply.note);
        let content_type = Header::from_bytes("Content-Type", reply.content_type).expect("en-tête valide");
        let response = Response::from_string(reply.body).with_status_code(reply.status).with_header(content_type);
        if let Err(e) = request.respond(response) {
            eprintln!("  réponse non envoyée : {e}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_from_text_lines() {
        let req: TodoRequest = serde_json::from_str(r#"{"text": "Pain\n\n  Lait  \nOeufs"}"#).unwrap();
        let ticket = req.into_ticket().unwrap();
        match &ticket.blocks[1] {
            Block::Todo { items, .. } => assert_eq!(items, &["Pain", "Lait", "Oeufs"]),
            _ => panic!("bloc todo attendu"),
        }
    }

    #[test]
    fn empty_todo_is_rejected() {
        let req: TodoRequest = serde_json::from_str(r#"{"text": "\n"}"#).unwrap();
        assert!(req.into_ticket().is_err());
    }

    #[test]
    fn token_comparison() {
        assert!(same_token("abc", "abc"));
        assert!(!same_token("abc", "abd"));
        assert!(!same_token("abc", "abcd"));
    }
}
