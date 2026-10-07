//! Revue de presse du jour, rédigée par Claude à partir de recherches web :
//! actualités d'un pays (`news`) ou internationales (`world_news`).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Ctx;
use crate::doc::{Align, Doc, Style};
use crate::{draw, fr};

/// À incrémenter quand le prompt change, pour ne pas resservir une revue en cache.
const PROMPT_VERSION: u32 = 1;

pub enum Scope<'a> {
    Country(&'a str),
    World,
}

#[derive(Deserialize, Serialize)]
struct Review {
    articles: Vec<Article>,
}

#[derive(Deserialize, Serialize)]
struct Article {
    titre: String,
    resume: String,
    source: String,
    url: String,
    mise_en_avant: bool,
}

/// Normalise une URL pour comparer celles de Claude à celles des résultats de recherche.
fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').trim_start_matches("https://").trim_start_matches("http://").to_lowercase()
}

pub fn build(ctx: &Ctx, scope: Scope, count: u8, qr: u8) -> Result<Doc> {
    let count = count.clamp(1, 6);
    let qr = qr.min(2).min(count);
    let (key_scope, title, subject) = match scope {
        Scope::Country(country) => (
            country.to_lowercase(),
            format!("Actualités · {country}"),
            format!("l'actualité de ce pays : {country} (politique, société, économie, culture, sport, sciences…)"),
        ),
        Scope::World => (
            "monde".to_owned(),
            "Actualités · Monde".to_owned(),
            "l'actualité internationale (grands événements dans le monde, géopolitique, économie, sciences…)"
                .to_owned(),
        ),
    };

    let cache = ctx.cache.as_ref();
    let key = format!("{}-news-v{PROMPT_VERSION}-{key_scope}-{count}-{qr}", ctx.today);
    let cached: Option<Review> = cache.filter(|_| !ctx.refresh).and_then(|c| c.get(&key));
    let from_cache = cached.is_some();
    let review = match cached {
        Some(review) => review,
        None => {
            let claude = ctx.claude.as_ref().context("ANTHROPIC_API_KEY manquante")?;
            let review = generate(claude, ctx, &subject, count, qr)?;
            if let Some(cache) = cache {
                cache.put(&key, &review);
            }
            review
        }
    };

    let mut doc = Doc::new();
    doc.from_cache = from_cache;
    doc.header(&title);

    // Numérotation des articles mis en avant, repris sous leurs QR codes.
    let mut featured: Vec<(usize, &Article)> = Vec::new();
    for article in review.articles.iter().take(count as usize) {
        doc.feed(1);
        let marker = if article.mise_en_avant && featured.len() < qr as usize && !article.url.is_empty() {
            featured.push((featured.len() + 1, article));
            format!("[{}] ", featured.len())
        } else {
            String::new()
        };
        doc.hanging(&marker, &article.titre, Style::default().bold());
        doc.text(&article.resume, Style::default());
        doc.text(&format!("- {}", article.source), Style::default().small().align(Align::Right));
    }

    if !featured.is_empty() {
        doc.feed(1);
        let urls: Vec<&str> = featured.iter().map(|(_, a)| a.url.as_str()).collect();
        doc.image(draw::qr_row(&urls)?);
        // Légendes centrées sous chaque QR code (deux demi-colonnes de 21 caractères).
        let half = Style::default().columns() / featured.len();
        let caption: String = featured
            .iter()
            .map(|(n, a)| {
                let label: String = format!("[{n}] {}", a.source).chars().take(half - 1).collect();
                format!("{label:^half$}")
            })
            .collect();
        doc.line(&caption, Style::default().small().center());
    }
    Ok(doc)
}

fn generate(claude: &crate::claude::Claude, ctx: &Ctx, subject: &str, count: u8, qr: u8) -> Result<Review> {
    let system = "Tu rédiges une courte revue de presse pour un ticket imprimé sur une petite imprimante \
                  thermique. Texte brut uniquement : ni emoji, ni markdown ; les accents français \
                  s'impriment correctement, utilise-les normalement. Ton factuel et neutre, sans \
                  sensationnalisme. N'invente rien : chaque sujet doit venir de tes recherches web, \
                  avec l'URL exacte de l'article source.";
    let prompt = format!(
        "Nous sommes le {}. Cherche sur le web les informations les plus importantes des dernières \
         24 heures concernant {subject}. Choisis {count} sujets distincts, du plus important au moins \
         important, en privilégiant des sources de presse reconnues.\n\
         - titre : court, 70 caractères au plus.\n\
         - resume : 2 phrases qui expliquent l'essentiel.\n\
         - source : le nom du média.\n\
         - url : le lien direct vers l'article.\n\
         - mise_en_avant : true pour les {qr} sujets les plus intéressants à lire en entier, \
         false pour les autres.",
        fr::long_date(ctx.today)
    );
    let schema = json!({
        "type": "object",
        "properties": {
            "articles": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "titre": { "type": "string" },
                        "resume": { "type": "string" },
                        "source": { "type": "string" },
                        "url": { "type": "string" },
                        "mise_en_avant": { "type": "boolean" },
                    },
                    "required": ["titre", "resume", "source", "url", "mise_en_avant"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["articles"],
        "additionalProperties": false,
    });
    let (mut review, found): (Review, Vec<String>) = claude.search(system, &prompt, schema, "medium", 5)?;
    anyhow::ensure!(!review.articles.is_empty(), "aucun article trouvé");

    // Pas de QR code vers une URL qui n'apparaît pas dans les résultats de recherche.
    let found: Vec<String> = found.iter().map(|u| normalize_url(u)).collect();
    for article in &mut review.articles {
        if !found.contains(&normalize_url(&article.url)) {
            article.url.clear();
        }
    }
    // Si Claude n'a rien mis en avant, on prend les premiers sujets qui ont un lien vérifié.
    if !review.articles.iter().any(|a| a.mise_en_avant && !a.url.is_empty()) {
        for article in review.articles.iter_mut().filter(|a| !a.url.is_empty()).take(qr as usize) {
            article.mise_en_avant = true;
        }
    }
    Ok(review)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_compare_loosely() {
        assert_eq!(normalize_url("https://www.Example.com/a/"), normalize_url("http://www.example.com/a"));
        assert_ne!(normalize_url("https://example.com/a"), normalize_url("https://example.com/b"));
    }
}
