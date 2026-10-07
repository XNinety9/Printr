//! Mot du jour : un mot français rare ou savoureux, choisi par Claude.

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Ctx;
use crate::claude::Claude;
use crate::doc::{Doc, Style};
use crate::fr;

/// Lettres initiales tirées selon le jour, pour varier les mots d'un jour à l'autre.
const LETTERS: &[u8] = b"ABCDEFGHIJLMNOPRSTUV";

/// Nombre de mots passés qu'on demande à Claude d'éviter.
const HISTORY: usize = 60;

#[derive(Deserialize, Serialize)]
struct Word {
    mot: String,
    nature: String,
    definition: String,
    exemple: String,
    etymologie: String,
}

impl Word {
    fn is_complete(&self) -> bool {
        [&self.mot, &self.nature, &self.definition, &self.exemple, &self.etymologie]
            .iter()
            .all(|f| !f.trim().is_empty())
    }
}

pub fn build(ctx: &Ctx) -> Result<Doc> {
    let date = ctx.today;
    let cache = ctx.cache.as_ref();
    let key = format!("{date}-mot-du-jour");
    let cached: Option<Word> = cache.filter(|_| !ctx.refresh).and_then(|c| c.get(&key)).filter(Word::is_complete);
    let from_cache = cached.is_some();
    let w = match cached {
        Some(w) => w,
        None if ctx.preview => return Ok(super::ai_placeholder("Mot du jour", "Le mot du jour")),
        None => {
            let claude = ctx.claude.as_ref().context("ANTHROPIC_API_KEY manquante")?;
            let past = cache.map(|c| c.history("mots-du-jour.txt", HISTORY)).unwrap_or_default();
            // Une réponse incomplète est rare : on retente une fois avant d'abandonner.
            let w = match generate(claude, date, &past)? {
                w if w.is_complete() => w,
                _ => generate(claude, date, &past)?,
            };
            anyhow::ensure!(w.is_complete(), "réponse de Claude incomplète");
            if let Some(cache) = cache {
                cache.put(&key, &w);
                cache.remember("mots-du-jour.txt", &w.mot, HISTORY);
            }
            w
        }
    };

    let mut doc = Doc::new();
    doc.from_cache = from_cache;
    doc.header("Mot du jour");
    doc.feed(1);
    // En double taille si le mot tient sur une ligne.
    let big = Style::default().bold().center().size(2);
    let title_style = if w.mot.chars().count() <= big.columns() { big } else { Style::default().bold().center() };
    doc.text(&w.mot, title_style);
    doc.text(&w.nature, Style::default().small().center());
    doc.feed(1);
    doc.text(&w.definition, Style::default());
    doc.feed(1);
    doc.text(&format!("« {} »", w.exemple.trim_matches(['«', '»', ' ', '"'])), Style::default().small());
    doc.feed(1);
    doc.hanging("Étymologie : ", &w.etymologie, Style::default().small());
    Ok(doc)
}

fn generate(claude: &Claude, date: NaiveDate, past: &[String]) -> Result<Word> {
    let avoid = if past.is_empty() {
        String::new()
    } else {
        format!("\nCes mots ont déjà été proposés, choisis-en un autre : {}.", past.join(", "))
    };
    let letter = LETTERS[date.ordinal0() as usize % LETTERS.len()] as char;

    let system = "Tu tiens la rubrique « mot du jour » d'un ticket imprimé sur une petite imprimante \
                  thermique. Texte brut uniquement : ni emoji, ni markdown ; les accents français \
                  s'impriment correctement, utilise-les normalement.";
    let prompt = format!(
        "Nous sommes le {}. Choisis un mot français réel, rare ou savoureux, qui commence par la \
         lettre {letter} et mérite d'être connu.\n\
         - mot : le mot seul.\n\
         - nature : par exemple « nom masculin », « adjectif ».\n\
         - definition : 1 à 2 phrases, exacte, comme dans un dictionnaire.\n\
         - exemple : une phrase d'exemple.\n\
         - etymologie : en une phrase courte.{avoid}",
        fr::long_date(date)
    );
    let schema = json!({
        "type": "object",
        "properties": {
            "mot": { "type": "string" },
            "nature": { "type": "string" },
            "definition": { "type": "string" },
            "exemple": { "type": "string" },
            "etymologie": { "type": "string" },
        },
        "required": ["mot", "nature", "definition", "exemple", "etymologie"],
        "additionalProperties": false,
    });
    claude.generate(system, &prompt, schema, "medium")
}
