//! Blocs paramétrables d'un ticket, décrits en JSON.

mod air_quality;
mod challenge;
mod crypto;
mod holidays;
mod horoscope;
mod maze;
mod moon;
mod on_this_day;
mod quote;
mod saint;
mod sudoku;
mod sun;
mod weather;
mod word;

use std::panic::AssertUnwindSafe;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::NaiveDate;
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
    Challenge {
        #[serde(default)]
        show_answer: bool,
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
        }
    }
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
            Block::Challenge { .. } => "défi du jour",
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
            Block::Countdown { label, .. } => Some(label.clone()),
            Block::Crypto { coins, .. } => Some(coins.join(", ")),
            Block::Sun { location } => Some(location.clone()),
            Block::Image { path, url, .. } => path.as_ref().or(url.as_ref()).map(|p| {
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
                let style = Style { bold: *bold, underline: *underline, reverse: *reverse, small: *small, size: 1, align: *align };
                doc.text(text, style.size(*size));
            }
            Block::Separator { style } => {
                doc.rule(style.unwrap_or('-'));
            }
            Block::Date {} => {
                doc.text(&fr::capitalize(&fr::long_date(ctx.today)), Style::default().bold().center());
            }
            Block::Feed { lines } => {
                doc.feed(*lines);
            }
            Block::Image { path, url, dither } => {
                let img = match (path, url) {
                    (Some(path), None) => raster::load(path.as_ref(), *dither)?,
                    (None, Some(url)) => {
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
                    _ => anyhow::bail!("indiquer soit `path`, soit `url`"),
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
            Block::Challenge { show_answer } => doc = challenge::build(ctx.today, *show_answer),
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

        let mut ticket = Doc::new();
        let mut reports = Vec::with_capacity(done.len());
        for (i, (doc, report)) in done.into_iter().map(|d| d.expect("chaque bloc a répondu")).enumerate() {
            if i > 0 {
                ticket.feed(self.spacing);
            }
            ticket.append(doc);
            reports.push(report);
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
                {"type": "challenge"}
            ]}"#,
        )
        .unwrap();
        assert!(matches!(t.blocks[0], Block::Sun { ref location } if location == "Lyon"));
        assert!(matches!(t.blocks[1], Block::Challenge { show_answer: false }));
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
        let ctx = Ctx { today: NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(), http: ureq::agent(), claude: None, cache: None, refresh: false };
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
