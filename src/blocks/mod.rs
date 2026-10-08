//! Blocs paramétrables d'un ticket, décrits en JSON.

mod air_quality;
pub mod barnum;
mod crypto;
mod glitch;
mod holidays;
mod horoscope;
mod maze;
mod moon;
mod news;
mod on_this_day;
mod picto;
mod quote;
mod riddle;
mod saint;
mod sudoku;
mod sun;
mod weather;
mod word;
mod word_search;
mod workout;

use std::panic::AssertUnwindSafe;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::NaiveDate;
use rand::RngExt;
use serde::Deserialize;

use crate::cache::Cache;
use crate::claude::Claude;
use crate::doc::{Align, Doc, Style};
use crate::{fr, raster};

fn yes() -> bool {
    true
}
fn one() -> u8 {
    1
}
fn two() -> u8 {
    2
}
fn three() -> u8 {
    3
}
fn default_coins() -> Vec<String> {
    vec!["bitcoin".to_owned(), "ethereum".to_owned()]
}
fn default_currency() -> String {
    "eur".to_owned()
}
fn default_max_age_hours() -> u32 {
    24
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    /// Coupe le papier à la fin.
    #[serde(default = "yes")]
    pub cut: bool,
    /// Lignes vides entre deux blocs.
    #[serde(default = "one")]
    pub spacing: u8,
    pub blocks: Vec<Block>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Title {
        text: String,
        #[serde(default = "two")]
        size: u8,
    },
    Text {
        text: String,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        underline: bool,
        #[serde(default)]
        reverse: bool,
        #[serde(default)]
        small: bool,
        #[serde(default = "one")]
        size: u8,
        #[serde(default)]
        align: Align,
    },
    /// Message étrange de la machine (ajouté aussi au hasard, voir `glitch::chance`).
    Glitch {
        #[serde(default)]
        seed: Option<u64>,
    },
    Separator {
        #[serde(default)]
        style: Option<char>,
    },
    Date {},
    Feed {
        #[serde(default = "one")]
        lines: u8,
    },
    Image {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        url: Option<String>,
        /// Photo envoyée depuis l'interface web (identifiant).
        #[serde(default)]
        upload: Option<String>,
        #[serde(default = "yes")]
        dither: bool,
    },
    Qr {
        data: String,
        #[serde(default)]
        size: Option<u8>,
        #[serde(default)]
        caption: Option<String>,
    },
    Weather {
        location: String,
        #[serde(default = "one")]
        days: u8,
    },
    Horoscope {
        sign: String,
        #[serde(default)]
        tone: horoscope::Tone,
    },
    Saint {},
    Todo {
        #[serde(default)]
        title: Option<String>,
        items: Vec<String>,
    },
    Sudoku {
        #[serde(default)]
        difficulty: sudoku::Difficulty,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    #[serde(alias = "mots_meles", alias = "mots_caches", alias = "mots_en_grille")]
    WordSearch {
        #[serde(default)]
        difficulty: sudoku::Difficulty,
        /// Thème des mots ; tiré du numéro de grille si absent.
        #[serde(default)]
        theme: Option<word_search::Theme>,
        /// Mots à cacher à la place d'un thème.
        #[serde(default)]
        words: Vec<String>,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    Maze {
        #[serde(default)]
        width: Option<u8>,
        #[serde(default)]
        height: Option<u8>,
        #[serde(default)]
        seed: Option<u64>,
    },
    WordOfTheDay {},
    Holidays {
        #[serde(default)]
        zone: holidays::Zone,
        #[serde(default = "one")]
        count: u8,
    },
    Moon {},
    Countdown {
        label: String,
        date: NaiveDate,
    },
    OnThisDay {
        #[serde(default = "three")]
        count: u8,
    },
    AirQuality {
        location: String,
    },
    Quote {},
    Crypto {
        #[serde(default = "default_coins")]
        coins: Vec<String>,
        #[serde(default = "default_currency")]
        currency: String,
    },
    Sun {
        location: String,
    },
    /// Horoscope hors ligne et gratuit, calculé sur le ciel réel par Barnum.
    Barnum {
        /// Signe (en français, accents facultatifs)…
        #[serde(default)]
        sign: Option<String>,
        /// … ou date de naissance, dont Barnum déduit le signe.
        #[serde(default)]
        birth_date: Option<NaiveDate>,
        /// Affiche la position des astres.
        #[serde(default)]
        sky: bool,
        /// Autre série de formules pour le même ciel (`--sel` de Barnum).
        #[serde(default)]
        variant: Option<String>,
    },
    /// Pictogramme dessiné : cœur, étoile, soleil, fleur, sourire.
    Picto {
        shape: picto::Shape,
        #[serde(default)]
        size: picto::Size,
        #[serde(default = "one")]
        count: u8,
    },
    /// Défi sportif du jour, sans équipement.
    #[serde(alias = "defi_sportif")]
    Workout {
        /// facile, moyen (défaut) ou difficile.
        #[serde(default)]
        level: workout::Level,
        #[serde(default)]
        number: Option<usize>,
    },
    /// Énigme du jour (devinette, charade, logique, calcul).
    #[serde(alias = "enigme")]
    Riddle {
        /// Limite à une famille : devinette, charade, logique, calcul.
        #[serde(default)]
        kind: Option<riddle::Kind>,
        /// Numéro d'une énigme précise (imprimé sous chaque énigme).
        #[serde(default)]
        number: Option<usize>,
        /// Réponse : envers (défaut), lendemain, dessous ou aucune.
        #[serde(default)]
        answer: riddle::Answer,
    },
    /// Revue de presse à partir de flux RSS, sans IA.
    News {
        /// Titre du bandeau : « Actualités · <title> ».
        title: String,
        feeds: Vec<String>,
        #[serde(default = "three")]
        count: u8,
        /// Nombre de QR codes vers les articles les mieux classés (0 à 2).
        #[serde(default = "two")]
        qr: u8,
        /// Thèmes à privilégier (mots ou expressions).
        #[serde(default)]
        themes: Vec<String>,
        /// Mots qui écartent un article.
        #[serde(default)]
        exclude: Vec<String>,
        #[serde(default = "default_max_age_hours")]
        max_age_hours: u32,
    },
}

/// Ressources partagées par les blocs.
pub struct Ctx {
    pub today: NaiveDate,
    pub http: ureq::Agent,
    pub claude: Option<Claude>,
    pub cache: Option<Cache>,
    /// Ignore le contenu du cache (mais le met à jour).
    pub refresh: bool,
    /// Aperçu : aucun appel payant (Claude) ; les blocs concernés affichent un texte d'attente.
    pub preview: bool,
}

impl Ctx {
    pub fn new(refresh: bool) -> Self {
        let http = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(20))).build().into();
        Self {
            today: chrono::Local::now().date_naive(),
            http,
            claude: Claude::from_env(),
            cache: Cache::open(),
            refresh,
            preview: false,
        }
    }

    pub fn for_preview(mut self) -> Self {
        self.preview = true;
        self
    }
}

/// Texte d'attente d'un bloc généré par Claude, en aperçu (le vrai texte coûte un appel).
pub fn ai_placeholder(title: &str, what: &str) -> Doc {
    let mut doc = Doc::new();
    doc.header(title);
    doc.feed(1);
    doc.text(&format!("{what} sera rédigé par Claude au moment de l'impression."), Style::default().center());
    doc
}

/// Bilan de la construction d'un bloc, pour la console et les réponses HTTP.
pub struct Report {
    pub label: String,
    pub elapsed: Duration,
    pub cached: bool,
    pub error: Option<String>,
}

impl Block {
    fn name(&self) -> &'static str {
        match self {
            Block::Title { .. } => "titre",
            Block::Text { .. } => "texte",
            Block::Glitch { .. } => "glitch",
            Block::Separator { .. } => "séparateur",
            Block::Date {} => "date",
            Block::Feed { .. } => "espace",
            Block::Image { .. } => "image",
            Block::Qr { .. } => "QR code",
            Block::Weather { .. } => "météo",
            Block::Horoscope { .. } => "horoscope",
            Block::Saint {} => "saint du jour",
            Block::Todo { .. } => "à faire",
            Block::Sudoku { .. } => "sudoku",
            Block::WordSearch { .. } => "mots mêlés",
            Block::Maze { .. } => "labyrinthe",
            Block::WordOfTheDay {} => "mot du jour",
            Block::Holidays { .. } => "jours fériés",
            Block::Moon {} => "lune",
            Block::Countdown { .. } => "compte à rebours",
            Block::OnThisDay { .. } => "éphéméride",
            Block::AirQuality { .. } => "qualité de l'air",
            Block::Quote {} => "citation",
            Block::Crypto { .. } => "crypto",
            Block::Sun { .. } => "lever/coucher du soleil",
            Block::Riddle { .. } => "énigme",
            Block::Workout { .. } => "défi sportif",
            Block::Picto { .. } => "pictogramme",
            Block::Barnum { .. } => "horoscope Barnum",
            Block::News { .. } => "actualités",
        }
    }

    /// Nom du bloc, précisé par son paramètre principal : « météo · Lyon ».
    pub fn label(&self) -> String {
        let detail = match self {
            Block::Title { text, .. } => Some(text.chars().take(24).collect()),
            Block::Weather { location, .. } | Block::AirQuality { location } => Some(location.clone()),
            Block::Horoscope { sign, tone } => Some(format!("{sign} ({})", tone.label())),
            Block::Sudoku { difficulty, solution, .. } => {
                Some(format!("{}{}", difficulty.label(), if *solution { ", solution" } else { "" }))
            }
            Block::WordSearch { difficulty, theme, words, solution, .. } => {
                let subject = match (words.is_empty(), theme) {
                    (false, _) => "mes mots, ",
                    (true, Some(t)) => &format!("{}, ", t.label()),
                    (true, None) => "",
                };
                Some(format!("{subject}{}{}", difficulty.label(), if *solution { ", solution" } else { "" }))
            }
            Block::Countdown { label, .. } => Some(label.clone()),
            Block::Crypto { coins, .. } => Some(coins.join(", ")),
            Block::Sun { location } => Some(location.clone()),
            Block::News { title, .. } => Some(title.clone()),
            Block::Workout { level, .. } => Some(level.label().to_owned()),
            Block::Picto { shape, .. } => Some(shape.label().to_owned()),
            Block::Barnum { sign, birth_date, .. } => {
                birth_date.map(|d| format!("né le {d}")).or_else(|| sign.clone())
            }
            Block::Image { path, url, upload, .. } => path.as_ref().or(url.as_ref()).or(upload.as_ref()).map(|p| {
                p.rsplit('/').next().unwrap_or(p).to_owned()
            }),
            _ => None,
        };
        match detail {
            Some(d) => format!("{} · {d}", self.name()),
            None => self.name().to_owned(),
        }
    }

    pub fn build(&self, ctx: &Ctx) -> Result<Doc> {
        let mut doc = Doc::new();
        match self {
            Block::Title { text, size } => {
                doc.text(text, Style::default().bold().center().size(*size));
            }
            Block::Text { text, bold, underline, reverse, small, size, align } => {
                let style = Style { bold: *bold, underline: *underline, reverse: *reverse, small: *small, align: *align, ..Style::default() };
                doc.text(text, style.size(*size));
            }
            Block::Glitch { seed } => doc = glitch::build(*seed),
            Block::Separator { style } => {
                doc.rule(style.unwrap_or('-'));
            }
            Block::Date {} => {
                doc.text(&fr::capitalize(&fr::long_date(ctx.today)), Style::default().bold().center());
            }
            Block::Feed { lines } => {
                doc.feed(*lines);
            }
            Block::Image { path, url, upload, dither } => {
                let img = match (path, url, upload) {
                    (None, None, Some(id)) => {
                        anyhow::ensure!(id.chars().all(|c| c.is_ascii_hexdigit()), "photo invalide");
                        let dir = crate::store::uploads_dir().context("répertoire de données introuvable")?;
                        raster::load(&dir.join(format!("{id}.png")), *dither)?
                    }
                    (Some(path), None, None) => raster::load(path.as_ref(), *dither)?,
                    (None, Some(url), None) => {
                        let bytes = ctx
                            .http
                            .get(url)
                            .call()
                            .context("image injoignable")?
                            .body_mut()
                            .with_config()
                            .limit(20 * 1024 * 1024)
                            .read_to_vec()?;
                        raster::prepare(&image::load_from_memory(&bytes)?, *dither)
                    }
                    _ => anyhow::bail!("indiquer `path`, `url` ou `upload` (un seul)"),
                };
                doc.image(img);
            }
            Block::Qr { data, size, caption } => {
                doc.qr(data, size.unwrap_or(6));
                if let Some(caption) = caption {
                    doc.text(caption, Style::default().small().center());
                }
            }
            Block::Weather { location, days } => doc = weather::build(&ctx.http, location, *days)?,
            Block::Horoscope { sign, tone } => doc = horoscope::build(ctx, sign, *tone)?,
            Block::Saint {} => doc = saint::build(ctx.today),
            Block::Todo { title, items } => {
                doc.header(title.as_deref().unwrap_or("À faire"));
                for item in items {
                    doc.hanging("[ ] ", item, Style::default());
                }
            }
            Block::Sudoku { difficulty, seed, solution } => doc = sudoku::build(*difficulty, *seed, *solution),
            Block::WordSearch { difficulty, theme, words, seed, solution } => {
                doc = word_search::build(*difficulty, *theme, words, *seed, *solution);
            }
            Block::Maze { width, height, seed } => {
                doc = maze::build(width.unwrap_or(12), height.unwrap_or(16), *seed);
            }
            Block::WordOfTheDay {} => doc = word::build(ctx)?,
            Block::Holidays { zone, count } => doc = holidays::build(ctx.today, *zone, *count),
            Block::Moon {} => doc = moon::build(chrono::Utc::now()),
            Block::Countdown { label, date } => {
                let days = (*date - ctx.today).num_days();
                let (big, small) = match days {
                    0 => ("C'est aujourd'hui !".to_owned(), label.clone()),
                    d if d > 0 => (format!("J-{d}"), format!("avant : {label}")),
                    d => (format!("J+{}", -d), format!("depuis : {label}")),
                };
                let size = if big.chars().count() <= 14 { 3 } else { 1 };
                doc.text(&big, Style::default().bold().center().size(size));
                doc.text(&small, Style::default().center());
            }
            Block::OnThisDay { count } => doc = on_this_day::build(&ctx.http, ctx.today, *count)?,
            Block::AirQuality { location } => doc = air_quality::build(&ctx.http, location)?,
            Block::Quote {} => doc = quote::build(ctx.today),
            Block::Crypto { coins, currency } => doc = crypto::build(&ctx.http, coins, currency)?,
            Block::Sun { location } => doc = sun::build(ctx, location)?,
            Block::Riddle { kind, number, answer } => doc = riddle::build(ctx.today, *kind, *number, *answer)?,
            Block::Workout { level, number } => doc = workout::build(ctx.today, *level, *number)?,
            Block::Picto { shape, size, count } => doc = picto::build(*shape, *size, *count),
            Block::Barnum { sign, birth_date, sky, variant } => {
                let who = match (birth_date, sign) {
                    (Some(birth), _) => barnum::Who::Birth(*birth),
                    (None, Some(sign)) => barnum::Who::Sign(sign.clone()),
                    (None, None) => anyhow::bail!("indiquer `sign` ou `birth_date`"),
                };
                doc = barnum::build(who, ctx.today, *sky, variant.as_deref())?;
            }
            Block::News { title, feeds, count, qr, themes, exclude, max_age_hours } => {
                let options = news::Options {
                    count: (*count).clamp(1, 5) as usize,
                    qr: *qr as usize,
                    themes,
                    exclude,
                    max_age: chrono::Duration::hours(*max_age_hours as i64),
                };
                doc = news::build(ctx, title, feeds, &options)?;
            }
        }
        Ok(doc)
    }
}

impl Ticket {
    /// Construit tous les blocs en parallèle (les requêtes réseau partent en même temps),
    /// puis les assemble dans l'ordre. Un bloc en échec est remplacé par un message.
    /// `progress` est prévenu au fil de l'eau, pour l'affichage en console.
    pub fn build(&self, ctx: &Ctx, progress: &mut dyn Progress) -> (Doc, Vec<Report>) {
        let labels: Vec<String> = self.blocks.iter().map(Block::label).collect();
        let mut done: Vec<Option<(Doc, Report)>> = self.blocks.iter().map(|_| None).collect();
        progress.start(&labels);

        std::thread::scope(|s| {
            let (tx, rx) = mpsc::channel();
            for (i, block) in self.blocks.iter().enumerate() {
                let tx = tx.clone();
                s.spawn(move || {
                    let start = Instant::now();
                    let result = std::panic::catch_unwind(AssertUnwindSafe(|| block.build(ctx)))
                        .unwrap_or_else(|_| Err(anyhow::anyhow!("erreur interne")))
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send((i, result, start.elapsed()));
                });
            }
            drop(tx);

            loop {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok((i, result, elapsed)) => {
                        let block = &self.blocks[i];
                        let (doc, error) = match result {
                            Ok(doc) => (doc, None),
                            Err(error) => {
                                let mut doc = Doc::new();
                                let message = format!("({} indisponible : {error})", block.name());
                                doc.text(&message, Style::default().small().center());
                                (doc, Some(error))
                            }
                        };
                        let report = Report { label: labels[i].clone(), elapsed, cached: doc.from_cache, error };
                        progress.done(i, &report);
                        done[i] = Some((doc, report));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => progress.tick(),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        progress.finish();

        // De temps en temps, la machine glisse un message entre deux blocs, sans rien dire en console.
        let forced = self.blocks.iter().any(|b| matches!(b, Block::Glitch { .. }));
        let glitch_at = (!ctx.preview && !forced && glitch::chance()).then(|| rand::rng().random_range(0..=done.len()));

        let mut ticket = Doc::new();
        let mut reports = Vec::with_capacity(done.len());
        let mut parts: Vec<Doc> = Vec::with_capacity(done.len() + 1);
        for (doc, report) in done.into_iter().map(|d| d.expect("chaque bloc a répondu")) {
            parts.push(doc);
            reports.push(report);
        }
        if let Some(at) = glitch_at {
            parts.insert(at, glitch::build(None));
        }
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                ticket.feed(self.spacing);
            }
            ticket.append(part);
        }
        (ticket, reports)
    }
}

/// Suivi de la construction d'un ticket (affichage en console).
pub trait Progress {
    fn start(&mut self, _labels: &[String]) {}
    /// Appelé régulièrement tant que des blocs sont en cours.
    fn tick(&mut self) {}
    fn done(&mut self, _index: usize, _report: &Report) {}
    fn finish(&mut self) {}
}

/// Aucun affichage (serveur, mode silencieux).
pub struct Silent;

impl Progress for Silent {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ticket_with_defaults() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [
                {"type": "title", "text": "Bonjour"},
                {"type": "saint"},
                {"type": "horoscope", "sign": "lion", "tone": "farfelu"},
                {"type": "sudoku", "difficulty": "facile"},
                {"type": "word_of_the_day"}
            ]}"#,
        )
        .unwrap();
        assert!(t.cut);
        assert_eq!(t.blocks.len(), 5);
    }

    #[test]
    fn parses_new_daily_blocks_and_defaults() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [
                {"type": "sun", "location": "Lyon"},
                {"type": "enigme", "kind": "charade", "answer": "lendemain"}
            ]}"#,
        )
        .unwrap();
        assert!(matches!(t.blocks[0], Block::Sun { ref location } if location == "Lyon"));
        assert!(matches!(t.blocks[1], Block::Riddle { kind: Some(riddle::Kind::Charade), .. }));
    }

    #[test]
    fn rejects_unknown_fields_and_types() {
        assert!(serde_json::from_str::<Ticket>(r#"{"blocks": [{"type": "saint", "oups": 1}]}"#).is_err());
        assert!(serde_json::from_str::<Ticket>(r#"{"blocks": [{"type": "licorne"}]}"#).is_err());
    }

    #[test]
    fn failing_block_does_not_break_ticket() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [{"type": "text", "text": "avant"}, {"type": "image", "path": "/nope.png"}, {"type": "text", "text": "après"}]}"#,
        )
        .unwrap();
        let ctx = Ctx { today: NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(), http: ureq::agent(), claude: None, cache: None, refresh: false, preview: false };
        let (doc, reports) = t.build(&ctx, &mut Silent);
        let preview = doc.preview(true);
        assert_eq!(reports.iter().filter(|r| r.error.is_some()).count(), 1);
        assert!(preview.contains("avant") && preview.contains("après"));
        assert!(preview.contains("image indisponible"));
    }

    #[test]
    fn build_reports_each_block_and_keeps_ticket_order() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [
                {"type": "title", "text": "premier"},
                {"type": "image", "path": "/nope.png"},
                {"type": "title", "text": "dernier"}
            ]}"#,
        )
        .unwrap();
        let ctx = Ctx {
            today: NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
            http: ureq::agent(),
            claude: None,
            cache: None,
            refresh: false,
            preview: false,
        };
        /// Note l'ordre dans lequel les blocs se terminent.
        struct Recorder(Vec<String>);
        impl Progress for Recorder {
            fn done(&mut self, _index: usize, report: &Report) {
                self.0.push(report.label.clone());
            }
        }
        let mut completed = Recorder(Vec::new());
        let (doc, reports) = t.build(&ctx, &mut completed);

        assert_eq!(completed.0.len(), 3);
        assert_eq!(
            reports
                .iter()
                .map(|report| report.label.as_str())
                .collect::<Vec<_>>(),
            ["titre · premier", "image · nope.png", "titre · dernier"]
        );
        let titles: Vec<_> = doc
            .ops
            .iter()
            .filter_map(|op| match op {
                crate::doc::Op::Line { text, .. } if text == "premier" || text == "dernier" => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(titles, ["premier", "dernier"]);
    }
}
