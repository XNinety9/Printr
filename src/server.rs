//! Serveur HTTP : interface web, API JSON et routes historiques pour les scripts.
//!
//! - Interface : `/` et ses fichiers (intégrés au binaire, ou lus dans `$PRINTR_WEB_DIR`).
//! - API web (`/api/…`) : session par cookie après connexion (prénom + mot de passe).
//! - Scripts : `POST /print` et `POST /todo` avec `Authorization: Bearer <jeton>`.
//!
//! Chaque requête est traitée dans son propre fil ; les impressions passent une à une.

use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::blocks::{Block, Ctx, Silent, Ticket};
use crate::guard::{self, LoginGuard};
use crate::i18n::t;
use crate::output::{Status, Target};
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
    /// Version plus récente publiée sur GitHub, trouvée par la vérification quotidienne.
    latest: Arc<Mutex<Option<crate::update::Available>>>,
    /// L'état de l'imprimante à la dernière vérification (voir `watch_printer`).
    printer_status: Mutex<Status>,
    /// Envoi resté en suspens chez l'imprimante (capot, papier) : tant qu'il n'est pas sorti, les
    /// tickets suivants attendent dans la file au lieu de s'y mêler.
    pending: Mutex<Option<Pending>>,
    /// Au-delà de ce délai, un ticket en attente est abandonné (le brief du matin n'a plus
    /// d'intérêt le soir) : `PRINTR_QUEUE_HOURS`, 12 heures par défaut.
    queue_hours: i64,
}

fn busy_reason() -> &'static str {
    t(
        "un ticket précédent attend encore l'imprimante (capot, papier ?)",
        "an earlier ticket is still waiting for the printer (cover, paper?)",
    )
}

/// Un envoi en suspens chez l'imprimante, et l'entrée d'historique à mettre à jour quand il sort.
struct Pending {
    done: crate::output::Done,
    /// Heure de l'entrée d'historique (aucune pour un ticket sorti de la file d'attente).
    at: Option<DateTime<Utc>>,
    label: String,
    since: DateTime<Utc>,
    /// Longueur du ticket, comptée sur le rouleau quand il sort.
    paper_mm: u32,
}

/// Résultat d'une impression, renvoyé à l'interface.
pub struct Printed {
    pub errors: Vec<String>,
    pub failure: Option<String>,
    /// Imprimante absente : le ticket attend son retour (la raison, pour le journal).
    pub queued: Option<String>,
    /// Pourquoi l'imprimante n'a pas pris le ticket (absente, capot ouvert, plus de papier).
    pub printer: Status,
    /// L'imprimante ne prend plus rien (capot, papier) : le ticket, envoyé ou mis en file derrière un
    /// envoi en suspens, sortira quand elle sera prête.
    pub stalled: bool,
}

impl Printed {
    /// L'issue en un mot, pour les domotiques : printed, queued, stalled ou failed.
    pub fn state(&self) -> &'static str {
        if self.failure.is_some() {
            "failed"
        } else if self.stalled {
            "stalled"
        } else if self.queued.is_some() {
            "queued"
        } else {
            "printed"
        }
    }
}

impl App {
    /// Refuse une impression de plus si `by` a déjà atteint son quota de l'heure. Les
    /// planifications ne comptent pas : elles sont réglées par les membres eux-mêmes.
    fn quota_exceeded(&self, by: &str) -> Option<Reply> {
        self.quota_message(by).map(|e| Reply::error(429, e))
    }

    /// La raison du refus si `by` a atteint son quota de l'heure (HTTP, MQTT).
    pub fn quota_message(&self, by: &str) -> Option<String> {
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
        Some(tr!(
            "{} impressions dans l'heure, c'est le maximum : réessaie dans {wait} min",
            "limit is {} prints per hour: try again in {wait} min",
            self.max_per_hour
        ))
    }

    /// Ajoute des articles à la liste de courses (80 caractères au plus chacun).
    pub fn add_shopping(&self, items: &[&str], by: &str) -> Result<()> {
        let mut store = self.lock_store();
        for text in items {
            let text: String = text.chars().take(80).collect();
            store.data.shopping.push(store::ShoppingItem {
                id: store::new_id(6),
                text,
                by: by.to_owned(),
                added: Utc::now(),
            });
        }
        store.save()
    }

    /// Un ticket enregistré et partagé, par son identifiant ou son nom (sans tenir compte de la casse) :
    /// ceux qu'un script ou une domotique peut imprimer.
    pub fn shared_preset(&self, wanted: &str) -> Option<Preset> {
        let store = self.lock_store();
        let shared = || store.data.presets.iter().filter(|p| p.shared);
        let wanted_lower = wanted.to_lowercase();
        shared().find(|p| p.id == wanted).or_else(|| shared().find(|p| p.name.to_lowercase() == wanted_lower)).cloned()
    }

    /// Un rouleau neuf est posé (longueur en mm, ou celle du précédent) : la jauge repart de zéro.
    pub fn new_roll(&self, roll_mm: Option<u32>) -> Result<Value> {
        let mut store = self.lock_store();
        store.data.paper.new_roll(roll_mm);
        store.save()?;
        Ok(paper_json(&store.data.paper))
    }

    /// Les tickets partagés, que les domotiques peuvent imprimer (un bouton chacun dans Home Assistant).
    pub fn shared_presets(&self) -> Vec<crate::hass::PresetButton> {
        let store = self.lock_store();
        store
            .data
            .presets
            .iter()
            .filter(|p| p.shared)
            .map(|p| crate::hass::PresetButton { id: p.id.clone(), name: p.name.clone() })
            .collect()
    }

    /// Longueur d'un ticket en attente, d'après son entrée d'historique.
    fn queued_paper_mm(&self, job: &str) -> u32 {
        let store = self.lock_store();
        store.data.history.iter().find(|e| e.queued.as_deref() == Some(job)).and_then(|e| e.paper_mm).unwrap_or(0)
    }

    pub fn printer_status(&self) -> Status {
        *self.printer_status.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Note le nouvel état de l'imprimante, et le dit dans le journal quand il change.
    fn set_printer_status(&self, status: Status) {
        let was = std::mem::replace(&mut *self.printer_status.lock().unwrap_or_else(|e| e.into_inner()), status);
        if status != was {
            match status {
                Status::Ready | Status::Missing => eprintln!("{}", status.message()),
                _ => eprintln!("{}", tr!("imprimante : {}", "printer: {}", status.message())),
            }
        }
        // Plus de papier, puis prête : on a remis un rouleau, neuf a priori.
        if was == Status::PaperOut && status == Status::Ready {
            let mut store = self.lock_store();
            store.data.paper.ran_out();
            store.data.paper.new_roll(None);
            let roll_m = store.data.paper.roll_mm / 1000;
            if let Err(e) = store.save() {
                eprintln!("{}", tr!("rouleau non enregistré : {e:#}", "roll not saved: {e:#}"));
            }
            eprintln!("{}", tr!("nouveau rouleau ({roll_m} m)", "new roll ({roll_m} m)"));
        }
    }

    /// L'état publié pour les domotiques (MQTT) : imprimante, file d'attente, dernier ticket.
    pub fn state(&self) -> Value {
        let stalled = self.pending.lock().unwrap_or_else(|e| e.into_inner()).is_some();
        let store = self.lock_store();
        let last = store.data.history.last().map(|e| json!({ "label": e.label, "by": e.by, "at": e.at, "ok": e.ok }));
        json!({
            "printer_ready": self.printer_status() == Status::Ready,
            "printer_status": self.printer_status().code(),
            "stalled": stalled,
            "queued": store.data.queue.len(),
            "paper": paper_json(&store.data.paper),
            "last_ticket": last,
            "version": env!("CARGO_PKG_VERSION"),
        })
    }

    /// Verrouille les données, après les avoir relues si le fichier a changé sur le disque
    /// (compte créé en ligne de commande pendant que le serveur tourne, par exemple).
    /// Imprime un ticket construit par Printr lui-même (configuration), sans historique.
    pub fn print_doc(&self, doc: &crate::doc::Doc) -> Result<()> {
        let bytes = crate::output::render(doc, true)?;
        let printer = self.printer.lock().unwrap_or_else(|e| e.into_inner());
        printer.write(&bytes)
    }

    pub fn lock_store(&self) -> std::sync::MutexGuard<'_, Store> {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = store.reload_if_changed() {
            eprintln!("{}", tr!("données non relues : {e:#}", "data not reloaded: {e:#}"));
        }
        store
    }

    /// Construit, imprime et note dans l'historique un ticket. Si l'imprimante est absente, le
    /// ticket, déjà mis en page, attend son retour dans la file.
    pub fn print(&self, ticket: &Ticket, by: &str, label: &str) -> Printed {
        let started = Utc::now();
        let (doc, reports) = ticket.build(&Ctx::web(false), &mut Silent);
        let errors: Vec<String> =
            reports.iter().filter_map(|r| r.error.as_ref().map(|e| format!("{} : {e}", r.label))).collect();
        let mut stalled = false;
        let mut busy = false;
        let at = Utc::now();
        let paper_mm = (doc.height_dots() as f64 * 25.4 / 180.0).round() as u32;
        // Gardé jusqu'à l'historique : un envoi en suspens y est noté avant qu'on puisse le solder.
        let printer = self.printer.lock().unwrap_or_else(|e| e.into_inner());
        let (failure, queued) = match crate::output::render(&doc, ticket.cut) {
            Err(e) => (Some(format!("{e:#}")), None),
            Ok(bytes) => {
                // Un ticket précédent encore en suspens : celui-ci attend son tour dans la file. Plus de
                // papier ou capot ouvert : il attend aussi, plutôt que de rester en suspens.
                busy = self.settle_pending();
                let status = if busy { None } else { printer.status() };
                if let Some(status) = status {
                    self.set_printer_status(status);
                }
                let sent = match status {
                    _ if busy => Err(anyhow::anyhow!("{}", busy_reason())),
                    Some(status) if status != Status::Ready => Err(anyhow::anyhow!("{}", status.message())),
                    _ => printer.write(&bytes),
                };
                match sent {
                    Ok(()) => (None, None),
                    // En suspens chez l'imprimante : il sortira tout seul, ne pas le remettre en file.
                    Err(e) if e.is::<crate::output::Stalled>() => {
                        stalled = true;
                        if let Some(done) = e.downcast_ref::<crate::output::Stalled>().and_then(|s| s.take()) {
                            *self.pending.lock().unwrap_or_else(|e| e.into_inner()) =
                                Some(Pending { done, at: Some(at), label: label.to_owned(), since: at, paper_mm });
                        }
                        (None, None)
                    }
                    Err(e) => {
                        if !busy && self.printer_status() == Status::Ready {
                            self.set_printer_status(Status::Missing);
                        }
                        let id = crate::store::new_id(8);
                        match crate::store::save_job(&id, &bytes) {
                            Ok(()) => (None, Some((id, format!("{e:#}")))),
                            Err(_) => (Some(format!("{e:#}")), None),
                        }
                    }
                }
            }
        };
        // Le ticket tel qu'imprimé (ou tel qu'il aurait dû l'être), pour l'aperçu de l'admin.
        let preview = crate::store::save_preview(&doc.to_json());
        let mut store = self.lock_store();
        if let Some((id, _)) = &queued {
            store.data.queue.push(crate::store::QueuedJob {
                id: id.clone(),
                at: Utc::now(),
                by: by.to_owned(),
                label: label.to_owned(),
            });
        }
        store.record(HistoryEntry {
            preview,
            queued: queued.as_ref().map(|(id, _)| id.clone()),
            printed_at: None,
            at,
            by: by.to_owned(),
            label: label.to_owned(),
            ok: failure.is_none() && queued.is_none(),
            errors: failure
                .iter()
                .chain(&errors)
                .cloned()
                .chain(stalled.then(|| crate::output::Stalled::message().to_owned()))
                .collect(),
            blocks: ticket.blocks.iter().map(|b| b.name().to_owned()).collect(),
            paper_mm: Some(paper_mm),
        });
        // Liste de courses imprimée avec `clear` : on retire ce qui y figurait (pas ce qui a été
        // ajouté pendant l'impression). Un ticket en attente ne vide rien : la liste peut encore servir.
        let printed_now = failure.is_none() && queued.is_none();
        // En suspens : compté quand il sortira (voir `settle_pending`).
        if printed_now && !stalled {
            store.data.paper.consume(paper_mm);
        }
        if printed_now && ticket.blocks.iter().any(|b| matches!(b, Block::Shopping { clear: true, .. })) {
            store.data.shopping.retain(|item| item.added > started);
        }
        if let Err(e) = store.save() {
            eprintln!("{}", tr!("historique non enregistré : {e:#}", "history not saved: {e:#}"));
        }
        drop(store);
        drop(printer);
        // Mis en file derrière un ticket en suspens : l'appli dit pourquoi (capot, papier).
        Printed {
            errors,
            failure,
            queued: queued.map(|(_, reason)| reason),
            printer: self.printer_status(),
            stalled: stalled || busy,
        }
    }

    /// Un envoi en suspens est-il enfin sorti (ou en échec) ? Met son entrée d'historique à jour.
    /// Vrai s'il attend encore. À appeler avec le verrou d'impression.
    fn settle_pending(&self) -> bool {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let Some(current) = pending.as_ref() else { return false };
        let outcome = match current.done.try_recv() {
            Err(TryRecvError::Empty) => return true,
            Ok(result) => result.map_err(|e| e.to_string()),
            Err(TryRecvError::Disconnected) => Err(t("écriture interrompue", "writing interrupted").to_owned()),
        };
        let Some(done) = pending.take() else { return false };
        drop(pending);
        {
            let mut store = self.lock_store();
            if let Some(entry) = store.data.history.iter_mut().find(|e| Some(e.at) == done.at && e.label == done.label)
            {
                entry.errors.retain(|m| m != crate::output::Stalled::message());
                match &outcome {
                    Ok(()) => entry.printed_at = Some(Utc::now()),
                    Err(reason) => {
                        entry.ok = false;
                        entry.errors.insert(0, reason.clone());
                    }
                }
            }
            if outcome.is_ok() {
                store.data.paper.consume(done.paper_mm);
            }
            if let Err(e) = store.save() {
                eprintln!("{}", tr!("historique non enregistré : {e:#}", "history not saved: {e:#}"));
            }
        }
        let waited = (Utc::now() - done.since).num_seconds();
        match outcome {
            Ok(()) => eprintln!(
                "{}",
                tr!("« {} » imprimé (en suspens {waited} s)", "\"{}\" printed (stalled {waited} s)", done.label)
            ),
            Err(reason) => eprintln!("« {} » : {reason}", done.label),
        }
        false
    }

    /// Vérifie que l'imprimante est là, et lui envoie les tickets qui l'attendaient. Les tickets
    /// trop anciens sont abandonnés et notés comme tels dans l'historique.
    fn watch_printer(&self) {
        let printer = self.printer.lock().unwrap_or_else(|e| e.into_inner());
        // Inconnu (envoi en cours) : on garde le dernier état.
        let status = printer.status().unwrap_or_else(|| self.printer_status());
        self.set_printer_status(status);
        let ready = status == Status::Ready;
        // Un envoi encore en suspens : la file attend qu'il soit sorti.
        if self.settle_pending() {
            return;
        }
        let jobs = self.lock_store().data.queue.clone();
        let limit = Utc::now() - chrono::Duration::hours(self.queue_hours);
        for job in jobs {
            // Le ticket précédent vient de passer en suspens : les suivants attendent.
            if self.pending.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                break;
            }
            // Sorti tout de suite (pas en suspens) : compté sur le rouleau.
            let mut out_now = false;
            let outcome = if job.at < limit {
                Err(tr!(
                    "abandonné : imprimante absente pendant plus de {} h",
                    "dropped: printer missing for more than {} h",
                    self.queue_hours
                ))
            } else if !ready {
                break;
            } else {
                match crate::store::load_job(&job.id) {
                    None => Err(t("ticket en attente perdu", "queued ticket lost").to_owned()),
                    Some(bytes) => match printer.write(&bytes) {
                        Ok(()) => {
                            out_now = true;
                            Ok(())
                        }
                        // Confié à l'imprimante, qui le sortira quand elle sera prête : les suivants
                        // attendent qu'il soit sorti.
                        Err(e) if e.is::<crate::output::Stalled>() => {
                            if let Some(done) = e.downcast_ref::<crate::output::Stalled>().and_then(|s| s.take()) {
                                *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending {
                                    done,
                                    at: None,
                                    label: job.label.clone(),
                                    since: Utc::now(),
                                    paper_mm: self.queued_paper_mm(&job.id),
                                });
                            }
                            Ok(())
                        }
                        // Toujours absente : on réessaiera au prochain tour.
                        Err(_) => {
                            self.set_printer_status(Status::Missing);
                            break;
                        }
                    },
                }
            };
            crate::store::remove_job(&job.id);
            let mut store = self.lock_store();
            store.data.queue.retain(|j| j.id != job.id);
            let mut paper_mm = 0;
            if let Some(entry) = store.data.history.iter_mut().find(|e| e.queued.as_deref() == Some(&job.id)) {
                entry.queued = None;
                paper_mm = entry.paper_mm.unwrap_or(0);
                match &outcome {
                    Ok(()) => {
                        entry.ok = entry.errors.is_empty();
                        entry.printed_at = Some(Utc::now());
                    }
                    Err(reason) => entry.errors.insert(0, reason.clone()),
                }
            }
            if out_now {
                store.data.paper.consume(paper_mm);
            }
            if let Err(e) = store.save() {
                eprintln!("{}", tr!("historique non enregistré : {e:#}", "history not saved: {e:#}"));
            }
            match outcome {
                Ok(()) => eprintln!(
                    "{}",
                    tr!(
                        "« {} » imprimé (en attente depuis {})",
                        "\"{}\" printed (queued since {})",
                        job.label,
                        job.at.with_timezone(&chrono::Local).format("%H:%M")
                    )
                ),
                Err(reason) => eprintln!("« {} » : {reason}", job.label),
            }
        }
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
    /// Même réponse, avec un autre code (202 : accepté, imprimé plus tard).
    fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

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
        .map_err(|e| Reply::error(400, tr!("corps illisible : {e}", "unreadable body: {e}")))?;
    if body.len() as u64 > limit {
        return Err(Reply::error(413, t("contenu trop volumineux", "content too large")));
    }
    Ok(body)
}

fn json_body<T: for<'de> Deserialize<'de>>(request: &mut Request) -> Result<T, Reply> {
    let body = read_body(request, MAX_JSON)?;
    serde_json::from_slice(&body).map_err(|e| Reply::error(400, tr!("JSON invalide : {e}", "invalid JSON: {e}")))
}

/// Nombre de blocs maximal d'un ticket venu du réseau : chacun peut lancer des requêtes et du
/// calcul, de quoi épuiser le Pi sans limite.
const MAX_BLOCKS: usize = 60;

/// Refuse un ticket trop long, d'où qu'il vienne (HTTP, MQTT).
pub fn ticket_too_long(ticket: &Ticket) -> Option<String> {
    (ticket.blocks.len() > MAX_BLOCKS).then(|| {
        tr!(
            "ticket trop long ({} blocs, {MAX_BLOCKS} au plus)",
            "ticket too long ({} blocks, {MAX_BLOCKS} max)",
            ticket.blocks.len()
        )
    })
}

fn check_ticket(ticket: Ticket) -> Result<Ticket, Reply> {
    match ticket_too_long(&ticket) {
        Some(e) => Err(Reply::error(400, e)),
        None => Ok(ticket),
    }
}

fn parse_ticket(value: &Value) -> Result<Ticket, Reply> {
    let ticket = serde_json::from_value(value.clone())
        .map_err(|e| Reply::error(400, tr!("ticket invalide : {e}", "invalid ticket: {e}")))?;
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
    if let (Some(token), Some(bearer)) = (&app.token, bearer)
        && same_secret(bearer.trim(), token)
    {
        return Some(Caller::Script);
    }
    let session = cookie(request, COOKIE)?;
    let store = app.lock_store();
    store.session_user(&session).cloned().map(Caller::User)
}

// ---- Fichiers de l'interface ----

mod examples {
    include!(concat!(env!("OUT_DIR"), "/examples.rs"));
}

/// Ticket d'exemple d'un bloc, pour les vignettes du catalogue : `/exemples/meteo.png`, ou
/// `/exemples/en/meteo.png` (en anglais, avec repli sur le français).
fn example(path: &str) -> Option<Vec<u8>> {
    let rest = path.strip_prefix("/exemples/")?;
    let (list, file) = match rest.strip_prefix("en/") {
        Some(file) => (examples::EXAMPLES_EN, file),
        None => (examples::EXAMPLES, rest),
    };
    let id = file.strip_suffix(".png")?;
    let find = |list: &[(&str, &[u8])]| list.iter().find(|(name, _)| *name == id).map(|(_, bytes)| bytes.to_vec());
    find(list).or_else(|| find(examples::EXAMPLES))
}

fn asset(app: &App, path: &str) -> Option<(Vec<u8>, &'static str)> {
    if let Some(png) = example(path) {
        return Some((png, "image/png"));
    }
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
        return std::fs::read(dir.join(name)).ok().map(|b| (localize(name, b), content_type));
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
    Some((localize(name, bytes.to_vec()), content_type))
}

/// La page et le manifeste sont écrits en français ; en anglais, le serveur change `<html lang>`
/// (l'interface s'en sert pour choisir sa langue) et les quelques textes vus avant son chargement.
fn localize(name: &str, bytes: Vec<u8>) -> Vec<u8> {
    if !crate::i18n::en() || !matches!(name, "index.html" | "manifest.webmanifest") {
        return bytes;
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    for (fr, en) in [
        (r#"<html lang="fr">"#, r#"<html lang="en">"#),
        (
            "Composer, imprimer et planifier des tickets sur l'imprimante de la maison.",
            "Compose, print and schedule tickets on the household receipt printer.",
        ),
        ("Printr a besoin de JavaScript.", "Printr needs JavaScript."),
        (r#""lang": "fr""#, r#""lang": "en""#),
    ] {
        text = text.replace(fr, en);
    }
    text.into_bytes()
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

/// Premier compte, avec la langue choisie sur l'écran d'accueil.
#[derive(Deserialize)]
struct SetupInput {
    name: String,
    password: String,
    #[serde(default)]
    lang: Option<String>,
}

/// La liste des anniversaires de la famille.
#[derive(Deserialize)]
struct BirthdaysInput {
    list: String,
}

/// Le Wi-Fi de la maison, choisi pendant la première connexion.
#[derive(Deserialize)]
struct WifiInput {
    ssid: String,
    #[serde(default)]
    password: String,
}

/// Imprimante à essayer depuis l'assistant, avant de l'enregistrer.
#[derive(Deserialize)]
struct TestPrint {
    #[serde(default)]
    device: Option<String>,
    #[serde(default)]
    tcp: Option<String>,
    #[serde(default)]
    pc850: bool,
}

/// Le ticket de test de l'assistant : des accents et un « € », pour voir si la page de code convient.
fn test_ticket_doc() -> crate::doc::Doc {
    use crate::doc::{Doc, Style};
    let mut doc = Doc::new();
    doc.text("Printr", Style::default().bold().center().size(2));
    doc.text(t("Ça marche !", "It works!"), Style::default().center());
    doc.feed(1);
    doc.text(t("Les accents doivent être lisibles :", "Accents should be readable:"), Style::default());
    doc.text("é è ê à â ç ù û ô î ï", Style::default().bold());
    doc.text("É È À Ç · 12,50 €", Style::default().bold());
    doc
}

#[derive(Deserialize)]
struct NewPassword {
    password: String,
}

/// Rouleau neuf posé ; sans longueur, la même que le précédent.
#[derive(Deserialize)]
struct NewRoll {
    #[serde(default)]
    length_m: Option<u32>,
}

/// La jauge de papier, pour l'appli et les domotiques.
fn paper_json(paper: &crate::store::Paper) -> Value {
    json!({
        "roll_mm": paper.roll_mm,
        "used_mm": paper.used_mm,
        "remaining_mm": paper.remaining_mm(),
        "percent": paper.percent(),
        "low": paper.low(),
        "since": paper.since,
    })
}

/// Réglages à enregistrer : nom de variable → valeur (vide : retirer le réglage).
#[derive(Deserialize)]
struct SettingsInput {
    values: std::collections::BTreeMap<String, String>,
}

/// Blocs à retirer du catalogue (types : « sudoku », « weather »…).
#[derive(Deserialize)]
struct HiddenBlocks {
    blocks: Vec<String>,
}

/// Favoris et récents d'un compte ; un champ absent reste tel quel.
#[derive(Deserialize)]
struct MyCatalog {
    favorites: Option<Vec<String>>,
    recent: Option<Vec<String>>,
}

const MAX_FAVORITES: usize = 100;
const MAX_RECENT: usize = 8;

/// Liste de blocs envoyée par l'appli : noms vérifiés, doublons retirés (le premier gardé), au plus
/// `max`. `None` si un nom n'en est pas un.
fn block_list(blocks: Vec<String>, max: usize) -> Option<Vec<String>> {
    if blocks.len() > 200 || !blocks.iter().all(|b| is_block_type(b)) {
        return None;
    }
    let mut seen = std::collections::HashSet::new();
    Some(blocks.into_iter().filter(|b| seen.insert(b.clone())).take(max).collect())
}

/// Un nom de type de bloc plausible : lettres minuscules et soulignés, rien d'autre à stocker.
fn is_block_type(name: &str) -> bool {
    (1..=40).contains(&name.len()) && name.bytes().all(|c| c.is_ascii_lowercase() || c == b'_')
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

/// Les articles d'un texte : un par ligne, sans les lignes vides.
pub fn shopping_items(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TodoRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    items: Vec<String>,
    /// Une chose à faire par ligne : pratique depuis un raccourci iOS.
    #[serde(default)]
    text: Option<String>,
}

impl TodoRequest {
    pub fn into_ticket(self) -> Result<Ticket> {
        let mut items = self.items;
        if let Some(text) = self.text {
            items.extend(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned));
        }
        if items.is_empty() {
            bail!("{}", t("aucune chose à faire (`items` ou `text`)", "nothing to do (`items` or `text`)"));
        }
        Ok(Ticket { cut: true, spacing: 1, blocks: vec![Block::Date { date: None, offset: None }, Block::Todo { title: self.title, items }] })
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
                bail!("{}", t("jours de planification invalides", "invalid schedule days"));
            }
            let valid_time = s
                .time
                .split_once(':')
                .and_then(|(h, m)| Some((h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)))
                .is_some_and(|(h, m)| h < 24 && m < 60);
            if !valid_time {
                bail!("{}", tr!("heure invalide : {}", "invalid time: {}", s.time));
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
    let dir = store::uploads_dir().context(t("répertoire de données introuvable", "data directory not found"))?;
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

    // Pendant la première connexion au réseau, le téléphone est renvoyé vers la page de configuration.
    if method == Method::Get
        && let Some(location) = crate::wifi::portal_redirect(header(request, "Host"))
    {
        return Reply::json(302, json!({ "ok": false })).header("Location", location);
    }
    if method == Method::Get {
        if let Some((body, content_type)) = asset(app, path) {
            return Reply { status: 200, content_type, body, headers: Vec::new(), note: String::new() }
                .header("Content-Security-Policy", CSP.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    } else if !guard::same_origin(header(request, "Origin"), header(request, "Host")) {
        return Reply::error(403, t("requête venue d'un autre site, refusée", "request from another site, refused"));
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
                return Reply::error(
                    429,
                    tr!("trop d'essais : réessaie dans {minutes} min", "too many attempts: try again in {minutes} min"),
                )
                .note(tr!("connexion bloquée pour {ip}", "login blocked for {ip}"));
            }
            // Argon2 est lent : la vérification se fait sans bloquer les données des autres.
            let hash = app.lock_store().password_hash(&login.user_id);
            if !hash.is_some_and(|h| app.guard.verify(&h, &login.password)) {
                app.guard.failed(ip);
                return Reply::error(401, t("mot de passe incorrect", "wrong password"));
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
                .note(tr!("connexion de {name}", "login: {name}"));
        }
        // Premier lancement : tant qu'aucun compte n'existe, l'écran d'accueil crée le premier.
        // Première connexion au réseau : sans compte au départ, depuis le point d'accès de Printr.
        (Method::Get, ["api", "wifi"]) => {
            return match crate::wifi::describe() {
                Some(state) => Reply::ok(state),
                None => Reply::error(404, t("pas de configuration Wi-Fi en cours", "no Wi-Fi setup in progress")),
            };
        }
        (Method::Post, ["api", "wifi"]) => {
            if client.is_some_and(crate::net::is_public) {
                return Reply::error(403, t("réservé au réseau local", "local network only"));
            }
            // Sans compte, même sur une installation déjà utilisée (Wi-Fi changé, déménagement) : le
            // point d'accès est protégé par le mot de passe imprimé sur le ticket, il faut donc avoir
            // l'imprimante sous la main. Et il n'existe que tant que Printr n'a pas de réseau.
            let input: WifiInput = try_reply!(json_body(request));
            return match crate::wifi::join(input.ssid.trim().to_owned(), input.password) {
                Ok(()) => Reply::ok(json!({ "ok": true })).note(t("connexion au Wi-Fi", "joining the Wi-Fi")),
                Err(e) => Reply::error(400, format!("{e:#}")),
            };
        }
        (Method::Post, ["api", "setup"]) => {
            // Depuis Internet, personne ne doit pouvoir s'approprier une installation vide.
            if client.is_some_and(crate::net::is_public) {
                return Reply::error(
                    403,
                    t(
                        "le premier compte se crée depuis le réseau de la maison",
                        "the first account must be created from the home network",
                    ),
                );
            }
            let input: SetupInput = try_reply!(json_body(request));
            let mut store = app.lock_store();
            if !store.users().is_empty() {
                return Reply::error(
                    403,
                    t("des comptes existent déjà : connecte-toi", "accounts already exist: log in"),
                );
            }
            let id = match store.add_user(&input.name, &input.password) {
                Ok(id) => id,
                Err(e) => return e.into(),
            };
            let token = store.open_session(&id);
            let user = store.user(&id).map(|u| user_json(&store, u));
            store.data.wizard_pending = true;
            // La langue choisie à l'accueil, si le système ne la fixe pas : elle demande un redémarrage.
            let lang = input.lang.as_deref().and_then(|l| crate::settings::check("PRINTR_LANG", l).ok());
            let restarting = match lang.filter(|l| !l.is_empty()) {
                Some(lang) if !crate::settings::is_locked("PRINTR_LANG") && lang != crate::i18n::lang().code() => {
                    store.data.settings.insert("PRINTR_LANG".to_owned(), lang);
                    crate::settings::restart_supported()
                }
                _ => false,
            };
            if let Err(e) = store.save() {
                return Reply::error(500, format!("{e:#}"));
            }
            if restarting {
                crate::settings::restart_soon();
            }
            return Reply::ok(json!({ "ok": true, "user": user, "restarting": restarting }))
                .header("Set-Cookie", session_cookie(&token, https))
                .note(tr!("premier compte : {}", "first account: {}", input.name.trim()));
        }
        _ => {}
    }

    let Some(caller) = caller(app, request) else {
        return if path.starts_with("/api/") || path == "/print" || path == "/todo" {
            Reply::error(401, t("connexion requise", "login required"))
        } else {
            Reply::error(404, t("page inconnue", "unknown page"))
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
                return Reply::error(403, t("réservé aux membres de la famille", "reserved for family members"));
            }
            let input: NewUser = try_reply!(json_body(request));
            let mut store = app.lock_store();
            let id = match store.add_user(&input.name, &input.password) {
                Ok(id) => id,
                Err(e) => return e.into(),
            };
            let user = store.user(&id).map(|u| user_json(&store, u));
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true, "user": user })).note(tr!(
                    "compte créé par {by} : {}",
                    "account created by {by}: {}",
                    input.name.trim()
                )),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Delete, ["api", "users", id]) => {
            if user_id.is_none() {
                return Reply::error(403, t("réservé aux membres de la famille", "reserved for family members"));
            }
            if user_id.as_deref() == Some(*id) {
                return Reply::error(
                    400,
                    t("tu ne peux pas supprimer ton propre compte", "you cannot delete your own account"),
                );
            }
            let mut store = app.lock_store();
            if !user_id.as_deref().is_some_and(|me| store.is_admin(me)) {
                return Reply::error(
                    403,
                    t("seul l'administrateur peut supprimer un compte", "only the administrator can delete an account"),
                );
            }
            let Some(name) = store.user(id).map(|u| u.name.clone()) else {
                return Reply::error(404, t("compte inconnu", "unknown account"));
            };
            store.remove_user_id(id);
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true }))
                    .note(tr!("compte {name} supprimé par {by}", "account {name} deleted by {by}")),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Get, ["api", "me", "catalog"]) => {
            let Caller::User(me) = &caller else {
                return Reply::error(403, t("réservé aux membres de la famille", "reserved for family members"));
            };
            let store = app.lock_store();
            let user = store.user(&me.id);
            Reply::ok(json!({
                "favorites": user.map(|u| u.favorites.clone()).unwrap_or_default(),
                "recent": user.map(|u| u.recent.clone()).unwrap_or_default(),
            }))
        }
        (Method::Put, ["api", "me", "catalog"]) => {
            let Caller::User(me) = &caller else {
                return Reply::error(403, t("réservé aux membres de la famille", "reserved for family members"));
            };
            let input: MyCatalog = try_reply!(json_body(request));
            let check = |list: Option<Vec<String>>, max| list.map(|b| block_list(b, max).ok_or(())).transpose();
            let (Ok(favorites), Ok(recent)) = (check(input.favorites, MAX_FAVORITES), check(input.recent, MAX_RECENT))
            else {
                return Reply::error(400, t("liste de blocs invalide", "invalid block list"));
            };
            let mut store = app.lock_store();
            let Some(user) = store.user_mut(&me.id) else {
                return Reply::error(404, t("compte inconnu", "unknown account"));
            };
            if let Some(favorites) = favorites {
                user.favorites = favorites;
            }
            if let Some(recent) = recent {
                user.recent = recent;
            }
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true })),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Put, ["api", "me", "password"]) => {
            let Caller::User(me) = &caller else {
                return Reply::error(403, t("réservé aux membres de la famille", "reserved for family members"));
            };
            let input: NewPassword = try_reply!(json_body(request));
            let mut store = app.lock_store();
            if let Err(e) = store.set_password(&me.name, &input.password) {
                return e.into();
            }
            // `set_password` ferme toutes les sessions : on en rouvre une pour cet appareil.
            let token = store.open_session(&me.id);
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true }))
                    .header("Set-Cookie", session_cookie(&token, https))
                    .note(tr!("mot de passe changé : {by}", "password changed: {by}")),
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
            let (queued, hidden_blocks, paper) = {
                let store = app.lock_store();
                (store.data.queue.len(), store.data.hidden_blocks.clone(), paper_json(&store.data.paper))
            };
            Reply::ok(json!({
                "printer": app.destination,
                "printer_ready": app.printer_status() == Status::Ready,
                "printer_status": app.printer_status().code(),
                "queued": queued,
                "paper": paper,
                "hidden_blocks": hidden_blocks,
                "claude": claude.as_ref().map(|c| c.model().to_owned()),
                "claude_backend": claude.as_ref().map(|c| c.backend()),
                "version": env!("CARGO_PKG_VERSION"),
                "mqtt_prefix": crate::mqtt::prefix(),
                "update": *app.latest.lock().unwrap_or_else(|e| e.into_inner()),
                "city": std::env::var("PRINTR_CITY").ok().filter(|c| !c.trim().is_empty()),
                "wizard": user_id.as_deref().is_some_and(|me| {
                    let store = app.lock_store();
                    store.data.wizard_pending && store.is_admin(me)
                }),
            }))
        }
        (Method::Post, ["api", "paper", "roll"]) => {
            let input: NewRoll = try_reply!(json_body(request));
            let roll_mm = input.length_m.map(|m| m.saturating_mul(1000));
            if roll_mm.is_some_and(|mm| !crate::store::Paper::ROLL_RANGE_MM.contains(&mm)) {
                return Reply::error(
                    400,
                    t("longueur de rouleau invalide (5 à 500 m)", "invalid roll length (5 to 500 m)"),
                );
            }
            match app.new_roll(roll_mm) {
                Ok(paper) => {
                    let roll_m = paper["roll_mm"].as_u64().unwrap_or(0) / 1000;
                    Reply::ok(json!({ "ok": true, "paper": paper }))
                        .note(tr!("nouveau rouleau ({roll_m} m) posé par {by}", "new roll ({roll_m} m) put in by {by}"))
                }
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Get, ["api", "setup", "printers"]) => {
            if !user_id.as_deref().is_some_and(|me| app.lock_store().is_admin(me)) {
                return Reply::error(403, t("réservé à l'administrateur", "administrator only"));
            }
            Reply::ok(crate::detect::printers())
        }
        (Method::Post, ["api", "setup", "test-print"]) => {
            let input: TestPrint = try_reply!(json_body(request));
            if !user_id.as_deref().is_some_and(|me| app.lock_store().is_admin(me)) {
                return Reply::error(403, t("réservé à l'administrateur", "administrator only"));
            }
            let target = match (&input.device, &input.tcp) {
                (Some(device), _) => match crate::settings::check("PRINTR_DEVICE", device) {
                    Ok(device) if !device.is_empty() => Target::Device(device.into()),
                    Ok(_) => return Reply::error(400, t("imprimante manquante", "missing printer")),
                    Err(e) => return Reply::error(400, format!("{e:#}")),
                },
                (None, Some(tcp)) => match crate::settings::check("PRINTR_TCP", tcp) {
                    // Seulement le réseau local : l'assistant ne doit pas servir à joindre Internet.
                    Ok(tcp)
                        if !tcp.is_empty()
                            && std::net::ToSocketAddrs::to_socket_addrs(tcp.as_str())
                                .ok()
                                .and_then(|mut a| a.next())
                                .is_some_and(|a| !crate::net::is_public(a.ip())) =>
                    {
                        Target::Tcp(tcp)
                    }
                    Ok(_) => {
                        return Reply::error(
                            400,
                            t("imprimante réseau locale attendue", "local network printer expected"),
                        );
                    }
                    Err(e) => return Reply::error(400, format!("{e:#}")),
                },
                (None, None) => return Reply::error(400, t("imprimante manquante", "missing printer")),
            };
            let bytes =
                match crate::cp858::with_codepage(input.pc850, || crate::output::render(&test_ticket_doc(), true)) {
                    Ok(bytes) => bytes,
                    Err(e) => return Reply::error(500, format!("{e:#}")),
                };
            // Une impression à la fois, même pour un essai.
            let _printer = app.printer.lock().unwrap_or_else(|e| e.into_inner());
            match target.write(&bytes) {
                Ok(()) => {
                    Reply::ok(json!({ "ok": true })).note(tr!("ticket de test sur {target}", "test ticket on {target}"))
                }
                Err(e) => Reply::error(502, format!("{e:#}")),
            }
        }
        (Method::Post, ["api", "wizard", "done"]) => {
            let mut store = app.lock_store();
            if !user_id.as_deref().is_some_and(|me| store.is_admin(me)) {
                return Reply::error(403, t("réservé à l'administrateur", "administrator only"));
            }
            store.data.wizard_pending = false;
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true })),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Get, ["api", "settings"]) => {
            if !user_id.as_deref().is_some_and(|me| app.lock_store().is_admin(me)) {
                return Reply::error(403, t("réservé à l'administrateur", "administrator only"));
            }
            Reply::ok(crate::settings::describe())
        }
        (Method::Put, ["api", "settings"]) => {
            let input: SettingsInput = try_reply!(json_body(request));
            let mut store = app.lock_store();
            if !user_id.as_deref().is_some_and(|me| store.is_admin(me)) {
                return Reply::error(403, t("réservé à l'administrateur", "administrator only"));
            }
            let mut changes = Vec::new();
            for (key, value) in &input.values {
                if crate::settings::is_locked(key) {
                    return Reply::error(
                        409,
                        tr!(
                            "{key} est fixé par le système (/etc/printr.env)",
                            "{key} is set by the system (/etc/printr.env)"
                        ),
                    );
                }
                match crate::settings::check(key, value) {
                    Ok(value) => changes.push((key.clone(), value)),
                    Err(e) => return Reply::error(400, format!("{e:#}")),
                }
            }
            for (key, value) in changes {
                if value.is_empty() {
                    store.data.settings.remove(&key);
                } else {
                    store.data.settings.insert(key, value);
                }
            }
            if let Err(e) = store.save() {
                return Reply::error(500, format!("{e:#}"));
            }
            drop(store);
            let restarting = crate::settings::restart_supported();
            if restarting {
                crate::settings::restart_soon();
            }
            Reply::ok(json!({ "ok": true, "restarting": restarting }))
                .note(tr!("réglages enregistrés par {by}", "settings saved by {by}"))
        }
        (Method::Put, ["api", "settings", "hidden-blocks"]) => {
            let input: HiddenBlocks = try_reply!(json_body(request));
            let mut store = app.lock_store();
            if !user_id.as_deref().is_some_and(|me| store.is_admin(me)) {
                return Reply::error(
                    403,
                    t(
                        "seul l'administrateur peut régler le catalogue",
                        "only the administrator can set up the catalog",
                    ),
                );
            }
            let mut blocks = input.blocks;
            if blocks.len() > 200 || blocks.iter().any(|b| !is_block_type(b)) {
                return Reply::error(400, t("liste de blocs invalide", "invalid block list"));
            }
            blocks.sort();
            blocks.dedup();
            let count = blocks.len();
            store.data.hidden_blocks = blocks;
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true })).note(tr!(
                    "catalogue : {count} bloc(s) masqué(s) par {by}",
                    "catalog: {count} block(s) hidden by {by}"
                )),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Post, ["api", "preview"]) => {
            let input: PrintRequest = try_reply!(json_body(request));
            let ticket = try_reply!(parse_ticket(&input.ticket));
            let (doc, reports) = ticket.build(&Ctx::web(false).for_preview(), &mut Silent);
            let mut out = doc.to_json();
            out["cut"] = json!(ticket.cut);
            out["reports"] = json!(
                reports
                    .iter()
                    .map(|r| json!({ "label": r.label, "error": r.error, "ms": r.elapsed.as_millis() as u64 }))
                    .collect::<Vec<_>>()
            );
            Reply::ok(out).note(tr!("aperçu · {} blocs", "preview · {} blocks", reports.len()))
        }
        (Method::Post, ["api", "print"]) => {
            let input: PrintRequest = try_reply!(json_body(request));
            let ticket = try_reply!(parse_ticket(&input.ticket));
            let label = input.label.unwrap_or_else(|| t("Ticket composé", "Composed ticket").to_owned());
            if let Some(refused) = app.quota_exceeded(&by) {
                return refused;
            }
            printed_reply(app.print(&ticket, &by, &label), &label)
        }
        (Method::Post, ["api", "images"]) => {
            let body = try_reply!(read_body(request, MAX_IMAGE));
            match save_upload(&body) {
                Ok(id) => {
                    Reply::ok(json!({ "id": id })).note(tr!("photo de {} Ko", "photo of {} KB", body.len() / 1024))
                }
                Err(e) => e.into(),
            }
        }
        (Method::Get, ["api", "images", id]) => {
            if !id.chars().all(|c| c.is_ascii_hexdigit()) {
                return Reply::error(404, t("photo inconnue", "unknown photo"));
            }
            match store::uploads_dir().map(|d| d.join(format!("{id}.png"))).and_then(|p| std::fs::read(p).ok()) {
                Some(body) => {
                    Reply { status: 200, content_type: "image/png", body, headers: Vec::new(), note: String::new() }
                        .header("Cache-Control", "private, max-age=31536000, immutable".to_owned())
                }
                None => Reply::error(404, t("photo inconnue", "unknown photo")),
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
            let Some(owner) = user_id else {
                return Reply::error(403, t("réservé aux utilisateurs", "reserved for users"));
            };
            let input: PresetInput = try_reply!(json_body(request));
            try_reply!(parse_ticket(&input.ticket));
            if input.name.trim().is_empty() {
                return Reply::error(400, t("donne un nom à ton preset", "give your preset a name"));
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
            let note = tr!("preset « {} » créé", "preset \"{}\" created", preset.name);
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
                return Reply::error(404, t("preset inconnu", "unknown preset"));
            };
            if Some(&store.data.presets[index].owner) != user_id.as_ref() {
                return Reply::error(
                    403,
                    t("seul son auteur peut modifier ce preset", "only its author can edit this preset"),
                );
            }
            let note;
            let out = match input {
                None => {
                    let removed = store.data.presets.remove(index);
                    note = tr!("preset « {} » supprimé", "preset \"{}\" deleted", removed.name);
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
                    note = tr!("preset « {} » modifié", "preset \"{}\" edited", preset.name);
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
                store
                    .data
                    .presets
                    .iter()
                    .find(|p| p.id == *id && (p.shared || Some(&p.owner) == user_id.as_ref()))
                    .cloned()
            };
            let Some(preset) = preset else { return Reply::error(404, t("preset inconnu", "unknown preset")) };
            let ticket = try_reply!(parse_ticket(&preset.ticket));
            if let Some(refused) = app.quota_exceeded(&by) {
                return refused;
            }
            printed_reply(app.print(&ticket, &by, &preset.name), &preset.name)
        }
        (Method::Get, ["api", "birthdays"]) => {
            let store = app.lock_store();
            Reply::ok(json!({ "list": store.data.family_birthdays }))
        }
        // La liste commune : tout le monde la complète.
        (Method::Put, ["api", "birthdays"]) => {
            let input: BirthdaysInput = try_reply!(json_body(request));
            let list = input.list.replace("\r\n", "\n");
            if list.len() > 10_000 || list.lines().count() > 200 || list.chars().any(|c| c.is_control() && c != '\n') {
                return Reply::error(400, t("liste trop longue ou invalide", "list too long or invalid"));
            }
            let mut store = app.lock_store();
            store.data.family_birthdays = list.trim().to_owned();
            match store.save() {
                Ok(()) => Reply::ok(json!({ "ok": true, "list": store.data.family_birthdays }))
                    .note(t("anniversaires de la famille", "family birthdays")),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Get, ["api", "shopping"]) => {
            let store = app.lock_store();
            Reply::ok(shopping_json(&store))
        }
        (Method::Post, ["api", "shopping"]) => {
            let input: ShoppingInput = try_reply!(json_body(request));
            let items = shopping_items(&input.text);
            if items.is_empty() {
                return Reply::error(400, t("rien à ajouter", "nothing to add"));
            }
            match app.add_shopping(&items, &by) {
                Ok(()) => Reply::ok(shopping_json(&app.lock_store())).note(tr!(
                    "courses : {}",
                    "shopping: {}",
                    items.join(", ")
                )),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        (Method::Delete, ["api", "shopping", rest @ ..]) => {
            let mut store = app.lock_store();
            let note = match rest {
                [] => {
                    store.data.shopping.clear();
                    t("liste de courses vidée", "shopping list cleared").to_owned()
                }
                [id] => {
                    store.data.shopping.retain(|i| i.id != *id);
                    t("article retiré", "item removed").to_owned()
                }
                _ => return Reply::error(404, t("route inconnue", "unknown route")),
            };
            match store.save() {
                Ok(()) => Reply::ok(shopping_json(&store)).note(note),
                Err(e) => Reply::error(500, format!("{e:#}")),
            }
        }
        // Aperçu d'un ticket de l'historique : l'administrateur seulement (pas le jeton des scripts).
        (Method::Get, ["api", "history", id, "preview"]) => {
            if !user_id.as_deref().is_some_and(|me| app.lock_store().is_admin(me)) {
                return Reply::error(403, t("réservé à l'administrateur", "reserved for the administrator"));
            }
            match crate::store::load_preview(id) {
                Some(preview) => Reply::ok(preview),
                None => Reply::error(
                    404,
                    t("aperçu indisponible (plus de 30 jours ?)", "preview unavailable (older than 30 days?)"),
                ),
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
                serde_json::from_slice(&body).context(t("ticket JSON invalide", "invalid ticket JSON"))
            } else {
                serde_json::from_slice::<TodoRequest>(&body)
                    .context(t("liste JSON invalide", "invalid list JSON"))
                    .and_then(TodoRequest::into_ticket)
            };
            let ticket = match ticket {
                Ok(t) => try_reply!(check_ticket(t)),
                Err(e) => return e.into(),
            };
            if preview {
                let (doc, _) = ticket.build(&Ctx::web(false).for_preview(), &mut Silent);
                let body = doc.preview(ticket.cut).into_bytes();
                return Reply {
                    status: 200,
                    content_type: "text/plain; charset=utf-8",
                    body,
                    headers: Vec::new(),
                    note: t("aperçu", "preview").into(),
                };
            }
            let label = if segments == ["todo"] {
                t("Liste à faire", "To-do list")
            } else {
                t("Ticket (script)", "Script ticket")
            };
            if let Some(refused) = app.quota_exceeded(&by) {
                return refused;
            }
            printed_reply(app.print(&ticket, &by, label), label)
        }
        _ => Reply::error(404, t("route inconnue", "unknown route")),
    }
}

fn printed_reply(printed: Printed, label: &str) -> Reply {
    if printed.stalled {
        return Reply::ok(json!({ "ok": true, "stalled": true, "errors": printed.errors }))
            .with_status(202)
            .note(tr!("« {label} » en suspens (capot, papier ?)", "\"{label}\" stalled (cover, paper?)"));
    }
    if let Some(reason) = printed.queued {
        return Reply::ok(json!({
            "ok": true,
            "queued": true,
            "printer_status": printed.printer.code(),
            "errors": printed.errors,
        }))
        .with_status(202)
        .note(tr!("« {label} » en attente ({reason})", "\"{label}\" queued ({reason})"));
    }
    match printed.failure {
        None => {
            let failed = if printed.errors.is_empty() {
                String::new()
            } else {
                tr!(" · {} bloc(s) en échec", " · {} block(s) failed", printed.errors.len())
            };
            Reply::ok(json!({ "ok": true, "errors": printed.errors }))
                .note(tr!("« {label} » imprimé{failed}", "\"{label}\" printed{failed}"))
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
        max_per_hour: std::env::var("PRINTR_MAX_TICKETS_PER_HOUR")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(DEFAULT_MAX_PER_HOUR),
        latest: Arc::new(Mutex::new(None)),
        printer_status: Mutex::new(Status::Ready),
        pending: Mutex::new(None),
        queue_hours: std::env::var("PRINTR_QUEUE_HOURS").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(12),
    });
    crate::update::spawn(Arc::clone(&app.latest));
    let server = Server::http(listen).map_err(|e| {
        anyhow::anyhow!("{}", tr!("impossible d'écouter sur {listen} : {e}", "cannot listen on {listen}: {e}"))
    })?;

    let ctx = Ctx::web(false);
    let cache = ctx.cache.as_ref().map(|c| c.dir().display().to_string());
    ui::banner(
        listen,
        &destination,
        ctx.claude.as_ref().map(|c| (c.model(), c.backend())),
        cache.as_deref(),
        &data,
        users,
    );

    crate::scheduler::spawn(Arc::clone(&app));
    crate::mqtt::spawn(Arc::clone(&app));
    crate::wifi::spawn(Arc::clone(&app), listen.rsplit(':').next().and_then(|p| p.parse().ok()).unwrap_or(8080));
    // L'imprimante est-elle là ? Toutes les 10 secondes ; c'est aussi ce qui vide la file d'attente.
    {
        let app = Arc::clone(&app);
        std::thread::spawn(move || {
            loop {
                app.watch_printer();
                std::thread::sleep(std::time::Duration::from_secs(10));
            }
        });
    }

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
    fn hidden_block_names_are_checked() {
        assert!(is_block_type("sudoku") && is_block_type("word_of_the_day"));
        assert!(!is_block_type("") && !is_block_type("Sudoku") && !is_block_type("../x") && !is_block_type("a b"));
        assert!(!is_block_type(&"a".repeat(41)));
    }

    #[test]
    fn block_lists_are_cleaned() {
        let list = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        assert_eq!(block_list(list(&["sudoku", "maze", "sudoku", "news"]), 2), Some(list(&["sudoku", "maze"])));
        assert_eq!(block_list(list(&["sudoku", "Bad name"]), 8), None);
        assert_eq!(block_list(Vec::new(), 8), Some(Vec::new()));
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
