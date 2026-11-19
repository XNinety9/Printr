//! Petit bac : une lettre tirée au hasard, des catégories, et de la place pour écrire.
//! Une feuille par joueur, toutes avec la même lettre, séparées par une ligne de découpe.

use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};

use crate::doc::{Align, Doc, Style};
use crate::i18n::{self, t};

/// Catégories en anglais, pour le jeu des catégories.
#[rustfmt::skip]
const CATEGORIES_EN: &[&str] = &[
    "First name",
    "Animal",
    "Country",
    "City",
    "Job",
    "Fruit or vegetable",
    "Object",
    "Color",
    "Brand",
    "Sport",
    "Dish",
    "Movie or show",
    "Singer or band",
    "Famous person",
    "Instrument",
    "Clothing",
    "Body part",
    "Means of transport",
    "Flower or plant",
    "Cartoon hero",
    "French word",
    "Kind insult",
    "Thing that stings",
    "Excuse for being late",
];

/// Lettres en anglais : sans Q, U, V, X, Y, Z, qui bloquent toute la partie.
#[rustfmt::skip]
const LETTERS_EN: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'R', 'S', 'T', 'W',
];

/// Catégories tirées au hasard quand le ticket n'en précise pas.
#[rustfmt::skip]
const CATEGORIES: &[&str] = &[
    "Prénom",
    "Animal",
    "Pays",
    "Ville",
    "Métier",
    "Fruit ou légume",
    "Objet",
    "Couleur",
    "Marque",
    "Sport",
    "Plat",
    "Film ou série",
    "Chanteur ou groupe",
    "Personnage célèbre",
    "Instrument",
    "Vêtement",
    "Partie du corps",
    "Moyen de transport",
    "Fleur ou plante",
    "Héros de dessin animé",
    "Mot anglais",
    "Insulte gentille",
    "Chose qui pique",
    "Excuse pour être en retard",
];

/// Lettres tirées par défaut : on écarte celles qui bloquent toute la partie.
#[rustfmt::skip]
const LETTERS: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'L', 'M', 'N', 'O', 'P', 'R', 'S', 'T', 'V',
];

/// Lettres tirées par défaut, selon la langue en vigueur.
fn letters() -> &'static [char] {
    if i18n::en() { LETTERS_EN } else { LETTERS }
}

/// Ligne « Catégorie ...... » : le reste de la ligne sert à écrire.
fn line(category: &str, width: usize) -> String {
    let dots = width.saturating_sub(category.chars().count() + 1);
    format!("{category} {}", ".".repeat(dots))
}

pub fn build(letter: Option<char>, categories: &[String], count: u8, players: u8, seed: Option<u64>) -> Doc {
    let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
    let mut rng = StdRng::seed_from_u64(seed);
    let letter = letter
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .unwrap_or_else(|| *letters().choose(&mut rng).expect("lettres"));
    let categories: Vec<String> = if categories.is_empty() {
        let mut pool = if i18n::en() { CATEGORIES_EN.to_vec() } else { CATEGORIES.to_vec() };
        pool.shuffle(&mut rng);
        pool.into_iter().take(count.clamp(3, 12) as usize).map(str::to_owned).collect()
    } else {
        categories.to_vec()
    };

    let width = Style::default().columns();
    let mut doc = Doc::new();
    for player in 0..players.clamp(1, 6) {
        if player > 0 {
            doc.feed(1);
            doc.line(&"- ".repeat(width / 2), Style::default());
            doc.feed(1);
        }
        doc.header(t("Petit bac", "Categories"));
        doc.feed(1);
        doc.text(t("La lettre est", "The letter is"), Style::default().center());
        doc.text(&letter.to_string(), Style::default().bold().center().size(5));
        doc.feed(1);
        for category in &categories {
            doc.line(&line(category, width), Style::default());
            doc.feed(1);
        }
        doc.line(&format!("{:>width$}", t("Points : ......", "Points: ......")), Style::default().bold());
        doc.text(&tr!("n° {seed}", "No. {seed}"), Style::default().small().align(Align::Right));
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Op;

    #[test]
    fn same_letter_on_every_sheet_and_lines_fit() {
        let doc = build(None, &[], 6, 3, Some(7));
        let big: Vec<&str> = doc
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Line { text, style } if style.size == 5 => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(big.len(), 3);
        assert!(big.iter().all(|l| *l == big[0]));
        for op in &doc.ops {
            if let Op::Line { text, style } = op {
                assert!(text.chars().count() <= style.columns(), "{text}");
            }
        }
    }

    #[test]
    fn english_sheets_use_safe_letters() {
        use crate::i18n::{Lang, with_lang};
        with_lang(Lang::En, || {
            for seed in 0..50 {
                let doc = build(None, &[], 6, 2, Some(seed));
                let big: Vec<&str> = doc
                    .ops
                    .iter()
                    .filter_map(|op| match op {
                        Op::Line { text, style } if style.size == 5 => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(big.len(), 2);
                assert!(!"QUVXYZ".contains(big[0].trim()), "{}", big[0]);
                for op in &doc.ops {
                    if let Op::Line { text, style } = op {
                        assert!(text.chars().count() <= style.columns(), "{text}");
                    }
                }
            }
            let doc = build(None, &[], 3, 1, Some(4));
            let all: Vec<String> = doc
                .ops
                .iter()
                .filter_map(|op| match op {
                    Op::Line { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect();
            assert!(all.iter().any(|l| l.contains("The letter is")));
            assert!(all.iter().any(|l| l.contains("Points: ......")));
            assert!(all.iter().any(|l| l.contains("No. 4")));
        });
    }

    #[test]
    fn custom_letter_and_categories() {
        let doc = build(Some('k'), &["Dinosaure".to_owned()], 6, 1, Some(1));
        let all: Vec<String> = doc
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Line { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(all.iter().any(|l| l.trim() == "K"));
        assert!(all.iter().any(|l| l.starts_with("Dinosaure ....")));
    }
}
