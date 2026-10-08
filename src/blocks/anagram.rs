//! Mots mystères : les lettres d'un mot mélangées, le thème et la longueur en indice,
//! les réponses imprimées à l'envers en bas. Les mots viennent des thèmes des mots mêlés.

use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};

use super::word_search::{grid_form, Theme};
use crate::doc::{Align, Doc, Style};

/// Thèmes tirés au hasard : ceux qui parlent à toute la famille (les thèmes techniques
/// restent disponibles en les demandant).
const FAMILY: &[Theme] = &[
    Theme::Animaux,
    Theme::FruitsLegumes,
    Theme::Cuisine,
    Theme::Nature,
    Theme::Sport,
    Theme::Metiers,
    Theme::Maison,
    Theme::Voyage,
    Theme::Musique,
    Theme::Ecole,
];

/// Mélange les lettres jusqu'à obtenir autre chose que le mot de départ.
fn scramble(word: &str, rng: &mut StdRng) -> String {
    let mut letters: Vec<char> = word.chars().collect();
    for _ in 0..20 {
        letters.shuffle(rng);
        let candidate: String = letters.iter().collect();
        if candidate != word {
            return candidate;
        }
    }
    letters.iter().collect()
}

/// « CHAT » → « C H A T » : plus lisible, et ça tient sur 21 colonnes en double taille.
fn spaced(word: &str) -> String {
    word.chars().map(String::from).collect::<Vec<_>>().join(" ")
}

pub fn build(theme: Option<Theme>, count: u8, seed: Option<u64>) -> Doc {
    let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
    let mut rng = StdRng::seed_from_u64(seed);
    let count = count.clamp(1, 6) as usize;

    // Chaque mot a son thème (tiré au hasard si aucun n'est imposé), sans doublon.
    let mut picked: Vec<(Theme, String, &str)> = Vec::new();
    while picked.len() < count {
        let theme = theme.unwrap_or_else(|| *FAMILY.choose(&mut rng).expect("thèmes"));
        let word = *theme.words().choose(&mut rng).expect("mots");
        let form = grid_form(word);
        // Au moins 5 lettres (sinon trop facile), au plus 10 (une ligne en double taille).
        if (5..=10).contains(&form.len()) && !picked.iter().any(|(_, f, _)| *f == form) {
            picked.push((theme, form, word));
        }
    }

    let mut doc = Doc::new();
    doc.header(if count > 1 { "Mots mystères" } else { "Mot mystère" });
    for (i, (theme, form, _)) in picked.iter().enumerate() {
        doc.feed(1);
        doc.text(&spaced(&scramble(form, &mut rng)), Style::default().bold().center().size(2));
        let number = if count > 1 { format!("{}. ", i + 1) } else { String::new() };
        let hint = format!("{number}{} · {} lettres", theme.label(), form.chars().count());
        doc.text(&hint, Style::default().small().center());
        doc.line(&format!("{:^42}", "_ ".repeat(form.len()).trim_end()), Style::default());
    }
    doc.text(&format!("n° {seed}"), Style::default().small().align(Align::Right));
    doc.feed(1);
    let answers: Vec<String> = picked
        .iter()
        .enumerate()
        .map(|(i, (_, _, word))| if count > 1 { format!("{}. {word}", i + 1) } else { word.to_string() })
        .collect();
    doc.text(&format!("Réponse{} : {}", if count > 1 { "s" } else { "" }, answers.join("  ")), Style::default().small().upside_down());
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Op;

    #[test]
    fn scrambled_words_use_the_same_letters() {
        let mut rng = StdRng::seed_from_u64(1);
        for word in ["CHEVAL", "ABRICOT", "GUITARE"] {
            let s = scramble(word, &mut rng);
            assert_ne!(s, word);
            let (mut a, mut b): (Vec<char>, Vec<char>) = (s.chars().collect(), word.chars().collect());
            a.sort();
            b.sort();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn lines_fit_for_many_seeds() {
        for seed in 0..100 {
            let doc = build(None, 4, Some(seed));
            for op in &doc.ops {
                if let Op::Line { text, style } = op {
                    assert!(text.chars().count() <= style.columns(), "{text}");
                }
            }
        }
    }
}
