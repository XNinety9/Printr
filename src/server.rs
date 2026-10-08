//! Serveur HTTP : interface web, API JSON et routes historiques pour les scripts.
//!
//! - Interface : `/` et ses fichiers (intégrés au binaire, ou lus dans `$PRINTR_WEB_DIR`).
//! - API web (`/api/…`) : session par cookie après connexion (prénom + mot de passe).
//! - Scripts : `POST /print` et `POST /todo` avec `Authorization: Bearer <jeton>`.
//!
//! Chaque requête est traitée dans son propre fil ; les impressions passent une à une.

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::blocks::{Block, Ctx, Silent, Ticket};
use crate::guard::{self, LoginGuard};
use crate::output::Target;
use crate::store::{self, HistoryEntry, Preset, Schedule, Store, User};
use crate::ui;

/// Taille maximale d'un corps JSON, et d'une photo envoyée.
const MAX_JSON: u64 = 1024 * 1024;
const MAX_IMAGE: u64 = 15 * 1024 * 1024;
const COOKIE: &str = "printr_session";
/// Impressions par personne et par heure, par défaut (`PRINTR_MAX_TICKETS_PER_HOUR`).
const DEFAULT_MAX_PER_HOUR: usize = 20;
/// Politique de sécurité du contenu de l'interface : rien d'autre que ses propres fichiers.
const CSP: &str = "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; \
                   script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

pub struct App {
    pub store: Mutex<Store>,
    /// Verrou d'impression : un seul ticket à la fois.
    printer: Mutex<Target>,
    destination: String,
    token: Option<String>,
    web_dir: Option<PathBuf>,
    guard: LoginGuard,
    /// Impressions permises par personne et par heure (0 : sans limite).
    max_per_hour: usize,
}

/// Résultat d'une impression, renvoyé à l'interface.
pub struct Printed {
    pub errors: Vec<String>,
    pub failure: Option<String>,
}

impl App {
    /// Refuse une impression de plus si `by` a déjà atteint son quota de l'heure. Les
    /// planifications ne comptent pas : elles sont réglées par les membres eux-mêmes.
    fn quota_exceeded(&self, by: &str) -> Option<Reply> {
        if self.max_per_hour == 0 || by == "planification" {
            return None;
        }
        let since = Utc::now() - chrono::Duration::hours(1);
        let store = self.lock_store();
        let recent: Vec<_> = store.data.history.iter().filter(|e| e.by == by && e.at > since).collect();
        if recent.len() < self.max_per_hour {
            return None;
        }
        let wait = (recent[0].at + chrono::Duration::hours(1) - Utc::now()).num_minutes().max(1);
        Some(Reply::error(429, format!("{} impressions dans l'heure, c'est le maximum : réessaie dans {wait} min", self.max_per_hour)))
    }

    /// Verrouille les données, après les avoir relues si le fichier a changé sur le disque
    /// (compte créé en ligne de commande pendant que le serveur tourne, par exemple).
    pub fn lock_store(&self) -> std::sync::MutexGuard<'_, Store> {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = store.reload_if_changed() {
            eprintln!("données non relues : {e:#}");
        }
        store
    }

    /// Construit, imprime et note dans l'historique un ticket.
    pub fn print(&self, ticket: &Ticket, by: &str, label: &str) -> Printed {
        let started = Utc::now();
        let (doc, reports) = ticket.build(&Ctx::web(false), &mut Silent);
        let errors: Vec<String> =
            reports.iter().filter_map(|r| r.error.as_ref().map(|e| format!("{} : {e}", r.label))).collect();
        let failure = {
            let printer = self.printer.lock().unwrap_or_else(|e| e.into_inner());
            printer.send(&doc, ticket.cut).err().map(|e| format!("{e:#}"))
        };
        let mut store = self.lock_store();
        store.record(HistoryEntry {
            at: Utc::now(),
            by: by.to_owned(),
            label: label.to_owned(),
            ok: failure.is_none(),
            errors: failure.iter().chain(&errors).cloned().collect(),
            blocks: ticket.blocks.iter().map(|b| b.name().to_owned()).collect(),
            paper_mm: Some((doc.height_dots() as f64 * 25.4 / 180.0).round() as u32),
        });
        // Liste de courses imprimée avec `clear` : on retire ce qui y figurait (pas ce qui a été
        // ajouté pendant l'impression).
        if failure.is_none() && ticket.blocks.iter().any(|b| matches!(b, Block::Shopping { clear: true, .. })) {
            store.data.shopping.retain(|item| item.added > started);
        }
        if let Err(e) = store.save() {
            eprintln!("historique non enregistré : {e:#}");
        }
        Printed { errors, failure }
    }
}

struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    headers: Vec<(String, String)>,
    /// Résumé pour le journal console.
    note: String,
}

impl Reply {
    fn json(status: u16, value: Value) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body: value.to_string().into_bytes(),
            headers: Vec::new(),
            note: String::new(),
        }
    }
    fn ok(value: Value) -> Self {
        Self::json(200, value)
    }
    fn error(status: u16, message: impl Into<String>) -> Self {
        let message = message.into();
        let mut reply = Self::json(status, json!({ "ok": false, "error": message }));
        reply.note = message;
        reply
    }
    fn note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }
    fn header(mut self, name: &str, value: String) -> Self {
        self.headers.push((name.to_owned(), value));
        self
    }
}

impl From<anyhow::Error> for Reply {
    fn from(e: anyhow::Error) -> Self {
        Reply::error(400, format!("{e:#}"))
    }
}

/// Comparaison en temps constant, pour ne pas révéler un secret par le temps de réponse.
fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn cookie(request: &Request, name: &str) -> Option<String> {
    header(request, "Cookie")?
        .split(';')
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
}

fn read_body(request: &mut Request, limit: u64) -> Result<Vec<u8>, Reply> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|e| Reply::error(400, format!("corps illisible : {e}")))?;
    if body.len() as u64 > limit {
        return Err(Reply::error(413, "contenu trop volumineux"));
    }
    Ok(body)
}

fn json_body<T: for<'de> Deserialize<'de>>(request: &mut Request) -> Result<T, Reply> {
    let body = read_body(request, MAX_JSON)?;
    serde_json::from_slice(&body).map_err(|e| Reply::error(400, format!("JSON invalide : {e}")))
}

/// Nombre de blocs maximal d'un ticket venu du réseau : chacun peut lancer des requêtes et du
/// calcul, de quoi épuiser le Pi sans limite.
const MAX_BLOCKS: usize = 60;

fn check_ticket(ticket: Ticket) -> Result<Ticket, Reply> {
    if ticket.blocks.len() > MAX_BLOCKS {
        return Err(Reply::error(400, format!("ticket trop long ({} blocs, {MAX_BLOCKS} au plus)", ticket.blocks.len())));
    }
    Ok(ticket)
}

fn parse_ticket(value: &Value) -> Result<Ticket, Reply> {
    let ticket = serde_json::from_value(value.clone()).map_err(|e| Reply::error(400, format!("ticket invalide : {e}")))?;
    check_ticket(ticket)
}

fn user_json(store: &Store, user: &User) -> Value {
    json!({ "id": user.id, "name": user.name, "color": user.color, "admin": store.is_admin(&user.id) })
}

/// Utilisateur connecté (cookie de session), ou script muni du jeton.
enum Caller {
    User(User),
    Script,
}

impl Caller {
    fn name(&self) -> String {
        match self {
            Caller::User(u) => u.name.clone(),
            Caller::Script => "script".to_owned(),
        }
    }
}

fn caller(app: &App, request: &Request) -> Option<Caller> {
    let bearer = header(request, "Authorization").and_then(|h| h.strip_prefix("Bearer "));
    if let (Some(token), Some(bearer)) = (&app.token, bearer) {
        if same_secret(bearer.trim(), token) {
            return Some(Caller::Script);
        }
    }
    let session = cookie(request, COOKIE)?;
    let store = app.lock_store();
    store.session_user(&session).cloned().map(Caller::User)
}

// ---- Fichiers de l'interface ----

fn asset(app: &App, path: &str) -> Option<(Vec<u8>, &'static str)> {
    let (name, content_type) = match path {
        "/" | "/index.html" => ("index.html", "text/html; charset=utf-8"),
        "/app.js" => ("app.js", "text/javascript; charset=utf-8"),
        "/style.css" => ("style.css", "text/css; charset=utf-8"),
        "/vendor/preact-htm.mjs" => ("vendor/preact-htm.mjs", "text/javascript; charset=utf-8"),
        "/favicon.png" => ("favicon.png", "image/png"),
        "/mark.png" => ("mark.png", "image/png"),
        "/icon-192.png" => ("icon-192.png", "image/png"),
        "/icon-512.png" => ("icon-512.png", "image/png"),
        "/apple-touch-icon.png" => ("apple-touch-icon.png", "image/png"),
        "/manifest.webmanifest" => ("manifest.webmanifest", "application/manifest+json"),
        _ => return None,
    };
    // En développement, `PRINTR_WEB_DIR` sert les fichiers depuis le disque (rechargement à chaud).
    if let Some(dir) = &app.web_dir {
        return std::fs::read(dir.join(name)).ok().map(|b| (b, content_type));
    }
    let bytes: &[u8] = match name {
        "index.html" => include_bytes!("../web/index.html"),
        "app.js" => include_bytes!("../web/app.js"),
        "style.css" => include_bytes!("../web/style.css"),
        "vendor/preact-htm.mjs" => include_bytes!("../web/vendor/preact-htm.mjs"),
        "favicon.png" => include_bytes!("../web/favicon.png"),
        "mark.png" => include_bytes!("../web/mark.png"),
        "icon-192.png" => include_bytes!("../web/icon-192.png"),
        "icon-512.png" => include_bytes!("../web/icon-512.png"),
        "apple-touch-icon.png" => include_bytes!("../web/apple-touch-icon.png"),
        _ => include_bytes!("../web/manifest.webmanifest"),
    };
    Some((bytes.to_vec(), content_type))
}

// ---- Corps des requêtes ----

#[derive(Deserialize)]
struct Login {
    user_id: String,
    password: String,
}

/// Nouveau compte : le premier (écran d'accueil) ou un membre de la famille.
#[derive(Deserialize)]
struct NewUser {
    name: String,
    password: String,
}

#[derive(Deserialize)]
struct NewPassword {
    password: String,
}

/// Cookie de session ; `Secure` quand la page est servie en HTTPS (derrière le reverse proxy).
fn session_cookie(token: &str, https: bool) -> String {
    let secure = if https { "; Secure" } else { "" };
    format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=15552000{secure}")
}

#[derive(Deserialize)]
struct PrintRequest {
    ticket: Value,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Deserialize)]
struct PresetInput {
    name: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    shared: bool,
    ticket: Value,
    #[serde(default)]
    schedules: Vec<ScheduleInput>,
}

#[derive(Deserialize)]
struct ScheduleInput {
    #[serde(default)]
    id: Option<String>,
    days: Vec<u8>,
    time: String,
    #[serde(default = "yes")]
    enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct ShoppingInput {
    /// Un article, ou plusieurs (un par ligne).
    text: String,
}

fn shopping_json(store: &Store) -> Value {
    json!(store.data.shopping)
}

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
        Ok(Ticket { cut: true, spacing: 1, blocks: vec![Block::Date {}, Block::Todo { title: self.title, items }] })
    }
}

fn validate_schedules(inputs: Vec<ScheduleInput>, previous: &[Schedule]) -> Result<Vec<Schedule>> {
    inputs
        .into_iter()
        .map(|s| {
            let mut days = s.days;
            days.sort_unstable();
            days.dedup();
            if days.is_empty() || days.iter().any(|d| !(1..=7).contains(d)) {
                bail!("jours de planification invalides");
            }
            let valid_time = s
                .time
                .split_once(':')
                .and_then(|(h, m)| Some((h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)))
                .is_some_and(|(h, m)| h < 24 && m < 60);
            if !valid_time {
                bail!("heure invalide : {}", s.time);
            }
            let id = s.id.unwrap_or_else(|| store::new_id(6));
            let last_run = previous.iter().find(|p| p.id == id).and_then(|p| p.last_run);
            Ok(Schedule { id, days, time: s.time, enabled: s.enabled, last_run })
        })
        .collect()
}

fn preset_json(preset: &Preset, store: &Store) -> Value {
    json!({
        "id": preset.id,
        "name": preset.name,
        "icon": preset.icon,
        "shared": preset.shared,
        "ticket": preset.ticket,
        "schedules": preset.schedules,
        "updated": preset.updated,
        "owner": store.user(&preset.owner).map(|u| user_json(store, u)),
    })
}

/// Enregistre une photo envoyée par l'interface : réduite à la largeur du papier, en PNG.
fn save_upload(bytes: &[u8]) -> Result<String> {
    let img = crate::raster::decode(bytes)?;
    let img = if img.width() > crate::raster::PRINT_WIDTH {
        img.resize(crate::raster::PRINT_WIDTH, u32::MAX, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let dir = store::uploads_dir().context("répertoire de données introuvable")?;
    std::fs::create_dir_all(&dir)?;
    let id = store::new_id(12);
    img.save_with_format(dir.join(format!("{id}.png")), image::ImageFormat::Png)?;
    Ok(id)
}

macro_rules! try_reply {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(r) => return r,
        }
    };
}

fn handle(app: &App, request: &mut Request) -> Reply {
    let url = request.url().to_owned();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let method = request.method().clone();
    let client = guard::client_ip(request.remote_addr().map(|a| a.ip()), header(request, "X-Forwarded-For"));
    let https = header(request, "X-Forwarded-Proto").is_some_and(|p| p.eq_ignore_ascii_case("https"));

    if method == Method::Get {
        if let Some((body, content_type)) = asset(app, path) {
            return Reply { status: 200, content_type, body, headers: Vec::new(), note: String::new() }
                .header("Content-Security-Policy", CSP.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    } else if !guard::same_origin(header(request, "Origin"), header(request, "Host")) {
        return Reply::error(403, "requête venue d'un autre site, refusée");
    }

    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    match (&method, segments.as_slice()) {
        // Routes publiques : écran de connexion.
        (Method::Get, ["api", "users"]) => {
            let store = app.lock_store();
            return Reply::ok(json!(store.users().iter().map(|u| user_json(&store, u)).collect::<Vec<_>>()));
        }
        (Method::Post, ["api", "login"]) => {
            let login: Login = try_reply!(json_body(request));
            let ip = client.unwrap_or(std::net::IpAddr::from([0, 0, 0, 0]));
            if let Err(wait) = app.guard.check(ip) {
                let minutes = wait.as_secs().div_ceil(60).max(1);
                return Reply::error(429, format!("trop d'essais : réessaie dans {minutes} min")).note(format!("connexion bloquée pour {ip}"));
            }
            // Argon2 est lent : la vérification se fait sans bloquer les données des autres.
            let hash = app.lock_store().password_hash(&login.user_id);
            if !hash.is_some_and(|h| app.guard.verify(&h, &login.password)) {
                app.guard.failed(ip);
                return Reply::error(401, "mot de passe incorrect");
            }
            app.guard.succeeded(ip);
            let mut store = app.lock_store();
            let token = store.open_session(&login.user_id);
            let user = store.user(&login.user_id).map(|u| user_json(&store, u));
            if let Err(e) = store.save() {
                return Reply::error(500, format!("{e:#}"));
            }
            let name = user.as_ref().and_then(|u| u["name"].as_str()).unwrap_or("?").to_owned();
            return Reply::ok(json!({ "ok": true, "user": user }))
                .header("Set-Cookie", session_cookie(&token, https))
                .note(format!("connexion de {name}"));
        }
        // Premier lancement : tant qu'aucun compte n'existe, l'écran d'accueil crée le premier.
        (Method::Post, ["api", "setup"]) => {
            // Depuis Internet, personne ne doit pouvoir s'approprier une installation vide.
            if client.is_some_and(crate::net::is_public) {
                return Reply::error(403, "le premier compte se crée depuis le réseau de la maison");
            }
            let input: NewUser = try_reply!(json_body(request));
            let mut store = app.lock_store();
            if !store.users().is_empty() {
                return Reply::error(403, "des comptes existent déjà : connecte-toi");
            }
            let id = match store.add_user(&input.name, &input.password) {
                Ok(id) => id,
                Err(e) => return e.into(),
            };
            let token = store.open_session(&id);
            let user = store.user(&id).map(|u| user_json(&store, u));
            if let Err(e) = store.save() {
                return Reply::error(500, format!("{e:#}"));
            }
            return Reply::ok(json!({ "ok": true, "user": user }))
                .header("Set-Cookie", session_cookie(&token, https))
                .note(format!("premier compte : {}", input.name.trim()));
        }
        _ => {}
    }

    let Some(caller) = caller(app, request) else {
        return if path.starts_with("/api/") || path == "/print" || path == "/todo" {
            Reply::error(401, "connexion requise")
        } else {
            Reply::error(404, "page inconnue")
        };
    };
    let by = caller.name();
    let user_id = match &caller {
        Caller::User(u) => Some(u.id.clone()),
        Caller::Script => None,
    };

    match (&method, segments.as_slice()) {
        (Method::Post, ["api", "logout"]) => {
            if let Some(token) = cookie(request, COOKIE) {
                let mut store = app.lock_store();
                store.logout(&token);
                let _ = store.save();
            }
            Reply::ok(json!({ "ok": true })).header("Set-Cookie", format!("{COOKIE}=; Path=/; Max-Age=0"))
        }
        // Comptes de la famille : tout membre connecté peut en ajouter ou en retirer.
        (Method::Post, ["api", "users"]) => {
            if user_id.is_none() {
                return Reply::error(403, "réservé aux membres de la famille");
            }
            let input: NewUser = try_reply!(json_body(request));
            let mut store = app.lock_store();
            let id = match store.add_user(&input.name, &input.password) {
                Ok(id) => id,
                Err(e) => return e.into(),
            };
            let user = store.user(&id).map(|u| user_json(&store, u));
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true, "user": user })).note(format!("compte créé par {by} : {}", input.name.trim())),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Delete, ["api", "users", id]) => {
            if user_id.is_none() {
                return Reply::error(403, "réservé aux membres de la famille");
            }
            if user_id.as_deref() == Some(*id) {
                return Reply::error(400, "tu ne peux pas supprimer ton propre compte");
            }
            let mut store = app.lock_store();
            if !user_id.as_deref().is_some_and(|me| store.is_admin(me)) {
                return Reply::error(403, "seul l'administrateur peut supprimer un compte");
            }
            let Some(name) = store.user(id).map(|u| u.name.clone()) else { return Reply::error(404, "compte inconnu") };
            store.remove_user_id(id);
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true })).note(format!("compte {name} supprimé par {by}")),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Put, ["api", "me", "password"]) => {
            let Caller::User(me) = &caller else { return Reply::error(403, "réservé aux membres de la famille") };
            let input: NewPassword = try_reply!(json_body(request));
            let mut store = app.lock_store();
            if let Err(e) = store.set_password(&me.name, &input.password) {
                return e.into();
            }
            // `set_password` ferme toutes les sessions : on en rouvre une pour cet appareil.
            let token = store.open_session(&me.id);
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true })).header("Set-Cookie", session_cookie(&token, https)).note(format!("mot de passe changé : {by}")),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Get, ["api", "me"]) => match &caller {
            Caller::User(u) => {
                let store = app.lock_store();
                Reply::ok(json!({ "user": user_json(&store, u) }))
            }
            Caller::Script => Reply::ok(json!({ "user": null })),
        },
        (Method::Get, ["api", "status"]) => {
            let claude = crate::claude::Claude::from_env();
            Reply::ok(json!({
                "printer": app.destination,
                "claude": claude.as_ref().map(|c| c.model().to_owned()),
                "version": env!("CARGO_PKG_VERSION"),
            }))
        }
        (Method::Post, ["api", "preview"]) => {
            let input: PrintRequest = try_reply!(json_body(request));
            let ticket = try_reply!(parse_ticket(&input.ticket));
            let (doc, reports) = ticket.build(&Ctx::web(false).for_preview(), &mut Silent);
            let mut out = doc.to_json();
            out["cut"] = json!(ticket.cut);
            out["reports"] = json!(reports
                .iter()
                .map(|r| json!({ "label": r.label, "error": r.error, "ms": r.elapsed.as_millis() as u64 }))
                .collect::<Vec<_>>());
            Reply::ok(out).note(format!("aperçu · {} blocs", reports.len()))
        }
        (Method::Post, ["api", "print"]) => {
            let input: PrintRequest = try_reply!(json_body(request));
            let ticket = try_reply!(parse_ticket(&input.ticket));
            let label = input.label.unwrap_or_else(|| "Ticket composé".to_owned());
            if let Some(refused) = app.quota_exceeded(&by) {
                return refused;
            }
            printed_reply(app.print(&ticket, &by, &label), &label)
        }
        (Method::Post, ["api", "images"]) => {
            let body = try_reply!(read_body(request, MAX_IMAGE));
            match save_upload(&body) {
                Ok(id) => Reply::ok(json!({ "id": id })).note(format!("photo de {} Ko", body.len() / 1024)),
                Err(e) => e.into(),
            }
        }
        (Method::Get, ["api", "images", id]) => {
            if !id.chars().all(|c| c.is_ascii_hexdigit()) {
                return Reply::error(404, "photo inconnue");
            }
            match store::uploads_dir().map(|d| d.join(format!("{id}.png"))).and_then(|p| std::fs::read(p).ok()) {
                Some(body) => Reply { status: 200, content_type: "image/png", body, headers: Vec::new(), note: String::new() }
                    .header("Cache-Control", "private, max-age=31536000, immutable".to_owned()),
                None => Reply::error(404, "photo inconnue"),
            }
        }
        (Method::Get, ["api", "presets"]) => {
            let store = app.lock_store();
            let visible: Vec<Value> = store
                .data
                .presets
                .iter()
                .filter(|p| p.shared || Some(&p.owner) == user_id.as_ref())
                .map(|p| preset_json(p, &store))
                .collect();
            Reply::ok(json!(visible))
        }
        (Method::Post, ["api", "presets"]) => {
            let Some(owner) = user_id else { return Reply::error(403, "réservé aux utilisateurs") };
            let input: PresetInput = try_reply!(json_body(request));
            try_reply!(parse_ticket(&input.ticket));
            if input.name.trim().is_empty() {
                return Reply::error(400, "donne un nom à ton preset");
            }
            let schedules = match validate_schedules(input.schedules, &[]) {
                Ok(s) => s,
                Err(e) => return e.into(),
            };
            let preset = Preset {
                id: store::new_id(8),
                owner,
                name: input.name.trim().to_owned(),
                icon: input.icon,
                shared: input.shared,
                ticket: input.ticket,
                schedules,
                updated: Utc::now(),
            };
            let mut store = app.lock_store();
            let out = preset_json(&preset, &store);
            let note = format!("preset « {} » créé", preset.name);
            store.data.presets.push(preset);
            match store.save() {
                Ok(()) => Reply::ok(out).note(note),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Put | Method::Delete, ["api", "presets", id]) => {
            let input: Option<PresetInput> =
                if method == Method::Put { Some(try_reply!(json_body(request))) } else { None };
            let mut store = app.lock_store();
            let Some(index) = store.data.presets.iter().position(|p| p.id == *id) else {
                return Reply::error(404, "preset inconnu");
            };
            if Some(&store.data.presets[index].owner) != user_id.as_ref() {
                return Reply::error(403, "seul son auteur peut modifier ce preset");
            }
            let note;
            let out = match input {
                None => {
                    let removed = store.data.presets.remove(index);
                    note = format!("preset « {} » supprimé", removed.name);
                    json!({ "ok": true })
                }
                Some(input) => {
                    try_reply!(parse_ticket(&input.ticket));
                    let previous = store.data.presets[index].schedules.clone();
                    let schedules = match validate_schedules(input.schedules, &previous) {
                        Ok(s) => s,
                        Err(e) => return e.into(),
                    };
                    let preset = &mut store.data.presets[index];
                    preset.name = input.name.trim().to_owned();
                    preset.icon = input.icon;
                    preset.shared = input.shared;
                    preset.ticket = input.ticket;
                    preset.schedules = schedules;
                    preset.updated = Utc::now();
                    note = format!("preset « {} » modifié", preset.name);
                    let preset = preset.clone();
                    preset_json(&preset, &store)
                }
            };
            match store.save() {
                Ok(()) => Reply::ok(out).note(note),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Post, ["api", "presets", id, "print"]) => {
            let preset = {
                let store = app.lock_store();
                store.data.presets.iter().find(|p| p.id == *id && (p.shared || Some(&p.owner) == user_id.as_ref())).cloned()
            };
            let Some(preset) = preset else { return Reply::error(404, "preset inconnu") };
            let ticket = try_reply!(parse_ticket(&preset.ticket));
            if let Some(refused) = app.quota_exceeded(&by) {
                return refused;
            }
            printed_reply(app.print(&ticket, &by, &preset.name), &preset.name)
        }
        (Method::Get, ["api", "shopping"]) => {
            let store = app.lock_store();
            Reply::ok(shopping_json(&store))
        }
        (Method::Post, ["api", "shopping"]) => {
            let input: ShoppingInput = try_reply!(json_body(request));
            let items: Vec<&str> = input.text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            if items.is_empty() {
                return Reply::error(400, "rien à ajouter");
            }
            let mut store = app.lock_store();
            for text in &items {
                let text: String = text.chars().take(80).collect();
                store.data.shopping.push(store::ShoppingItem { id: store::new_id(6), text, by: by.clone(), added: Utc::now() });
            }
            match store.save() {
                Ok(()) => Reply::ok(shopping_json(&store)).note(format!("courses : {}", items.join(", "))),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Delete, ["api", "shopping", rest @ ..]) => {
            let mut store = app.lock_store();
            let note = match rest {
                [] => {
                    store.data.shopping.clear();
                    "liste de courses vidée".to_owned()
                }
                [id] => {
                    store.data.shopping.retain(|i| i.id != *id);
                    "article retiré".to_owned()
                }
                _ => return Reply::error(404, "route inconnue"),
            };
            match store.save() {
                Ok(()) => Reply::ok(shopping_json(&store)).note(note),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Get, ["api", "history"]) => {
            let store = app.lock_store();
            let recent: Vec<&HistoryEntry> = store.data.history.iter().rev().take(100).collect();
            Reply::ok(json!(recent))
        }
        // Routes historiques pour les scripts et raccourcis.
        (Method::Post, ["print"]) | (Method::Post, ["todo"]) => {
            let preview = query.split('&').any(|p| p == "preview" || p.starts_with("preview="));
            let body = try_reply!(read_body(request, MAX_JSON));
            let ticket: Result<Ticket> = if segments == ["print"] {
                serde_json::from_slice(&body).context("ticket JSON invalide")
            } else {
                serde_json::from_slice::<TodoRequest>(&body)
                    .context("liste JSON invalide")
                    .and_then(TodoRequest::into_ticket)
            };
            let ticket = match ticket {
                Ok(t) => try_reply!(check_ticket(t)),
                Err(e) => return e.into(),
            };
            if preview {
                let (doc, _) = ticket.build(&Ctx::web(false).for_preview(), &mut Silent);
                let body = doc.preview(ticket.cut).into_bytes();
                return Reply { status: 200, content_type: "text/plain; charset=utf-8", body, headers: Vec::new(), note: "aperçu".into() };
            }
            let label = if segments == ["todo"] { "Liste à faire" } else { "Ticket (script)" };
            if let Some(refused) = app.quota_exceeded(&by) {
                return refused;
            }
            printed_reply(app.print(&ticket, &by, label), label)
        }
        _ => Reply::error(404, "route inconnue"),
    }
}

fn printed_reply(printed: Printed, label: &str) -> Reply {
    match printed.failure {
        None => {
            let failed = if printed.errors.is_empty() {
                String::new()
            } else {
                format!(" · {} bloc(s) en échec", printed.errors.len())
            };
            Reply::ok(json!({ "ok": true, "errors": printed.errors })).note(format!("« {label} » imprimé{failed}"))
        }
        Some(failure) => Reply::error(503, failure),
    }
}

/// Lance le serveur et le planificateur.
pub fn serve(listen: &str, token: Option<String>, target: Target) -> Result<()> {
    let store = Store::open()?;
    let users = store.users().len();
    let data = store.path().display().to_string();
    let destination = target.to_string();
    let app = Arc::new(App {
        store: Mutex::new(store),
        printer: Mutex::new(target),
        destination: destination.clone(),
        token,
        web_dir: std::env::var_os("PRINTR_WEB_DIR").map(PathBuf::from),
        guard: LoginGuard::default(),
        max_per_hour: std::env::var("PRINTR_MAX_TICKETS_PER_HOUR").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(DEFAULT_MAX_PER_HOUR),
    });
    let server = Server::http(listen).map_err(|e| anyhow::anyhow!("impossible d'écouter sur {listen} : {e}"))?;

    let ctx = Ctx::web(false);
    let cache = ctx.cache.as_ref().map(|c| c.dir().display().to_string());
    ui::banner(listen, &destination, ctx.claude.as_ref().map(|c| c.model()), cache.as_deref(), &data, users);

    crate::scheduler::spawn(Arc::clone(&app));

    for mut request in server.incoming_requests() {
        let app = Arc::clone(&app);
        std::thread::spawn(move || {
            let start = Instant::now();
            let reply = handle(&app, &mut request);
            let from = header(&request, "X-Forwarded-For")
                .map(|f| f.split(',').next().unwrap_or(f).trim().to_owned())
                .or_else(|| request.remote_addr().map(|a| a.ip().to_string()))
                .unwrap_or_else(|| "?".to_owned());
            let method = request.method().to_string();
            let path = request.url().split('?').next().unwrap_or("").to_owned();
            // Les fichiers de l'interface ne polluent pas le journal.
            if path.starts_with("/api/") || path == "/print" || path == "/todo" || reply.status >= 400 {
                ui::request(&method, &path, reply.status, &from, start.elapsed(), &reply.note);
            }
            let mut response = Response::from_data(reply.body).with_status_code(reply.status);
            if let Ok(h) = Header::from_bytes("Content-Type", reply.content_type) {
                response = response.with_header(h);
            }
            for (name, value) in reply.headers {
                if let Ok(h) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
                    response = response.with_header(h);
                }
            }
            let _ = request.respond(response);
        });
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
    fn secrets_compare_in_constant_time() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "abcd"));
    }

    #[test]
    fn schedules_are_validated() {
        let ok = vec![ScheduleInput { id: None, days: vec![5, 1, 1], time: "08:30".into(), enabled: true }];
        let s = validate_schedules(ok, &[]).unwrap();
        assert_eq!(s[0].days, vec![1, 5]);
        let bad_time = vec![ScheduleInput { id: None, days: vec![1], time: "25:00".into(), enabled: true }];
        assert!(validate_schedules(bad_time, &[]).is_err());
        let bad_day = vec![ScheduleInput { id: None, days: vec![8], time: "08:00".into(), enabled: true }];
        assert!(validate_schedules(bad_day, &[]).is_err());
    }
}
