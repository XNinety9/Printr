//! Petit bac : une lettre tirée au hasard, des catégories, et de la place pour écrire.
//! Une feuille par joueur, toutes avec la même lettre, séparées par une ligne de découpe.

use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};

use crate::doc::{Align, Doc, Style};

/// Catégories tirées au hasard quand le ticket n'en précise pas.
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
const LETTERS: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'L', 'M', 'N', 'O', 'P', 'R', 'S', 'T', 'V',
];

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
        .unwrap_or_else(|| *LETTERS.choose(&mut rng).expect("lettres"));
    let categories: Vec<String> = if categories.is_empty() {
        let mut pool = CATEGORIES.to_vec();
        pool.shuffle(&mut rng);
        pool.into_iter().take(count.clamp(3, 12) as usize).map(str::to_owned).collect()
    } else {
        categories.to_vec()
    };

    let mut doc = Doc::new();
    for player in 0..players.clamp(1, 6) {
        if player > 0 {
            doc.feed(1);
            doc.line(&"- ".repeat(21), Style::default());
            doc.feed(1);
        }
        doc.header("Petit bac");
        doc.feed(1);
        doc.text("La lettre est", Style::default().center());
        doc.text(&letter.to_string(), Style::default().bold().center().size(5));
        doc.feed(1);
        for category in &categories {
            doc.line(&line(category, 42), Style::default());
            doc.feed(1);
        }
        doc.line(&format!("{:>42}", "Points : ......"), Style::default().bold());
        doc.text(&format!("n° {seed}"), Style::default().small().align(Align::Right));
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
