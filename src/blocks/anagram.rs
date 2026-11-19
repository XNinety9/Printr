//! Mots mystères : les lettres d'un mot mélangées, le thème et la longueur en indice,
//! les réponses imprimées à l'envers en bas. Les mots viennent des thèmes des mots mêlés.

use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};

use super::word_search::{Theme, grid_form};
use crate::doc::{Align, Doc, Style};
use crate::i18n::t;

/// Thèmes tirés au hasard : ceux qui parlent à toute la famille (les thèmes techniques
/// restent disponibles en les demandant).
#[rustfmt::skip]
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
    doc.header(if count > 1 { t("Mots mystères", "Mystery words") } else { t("Mot mystère", "Mystery word") });
    for (i, (theme, form, _)) in picked.iter().enumerate() {
        doc.feed(1);
        doc.text(&spaced(&scramble(form, &mut rng)), Style::default().bold().center().size(2));
        let number = if count > 1 { format!("{}. ", i + 1) } else { String::new() };
        let letters = form.chars().count();
        let hint = tr!("{}{} · {} lettres", "{}{} · {} letters", number, theme.label(), letters);
        doc.text(&hint, Style::default().small().center());
        let width = Style::default().columns();
        doc.line(&format!("{:^width$}", "_ ".repeat(form.len()).trim_end()), Style::default());
    }
    doc.text(&tr!("n° {seed}", "No. {seed}"), Style::default().small().align(Align::Right));
    doc.feed(1);
    let answers: Vec<String> = picked
        .iter()
        .enumerate()
        .map(|(i, (_, _, word))| if count > 1 { format!("{}. {word}", i + 1) } else { word.to_string() })
        .collect();
    let plural = if count > 1 { "s" } else { "" };
    doc.text(
        &tr!("Réponse{} : {}", "Answer{}: {}", plural, answers.join("  ")),
        Style::default().small().upside_down(),
    );
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
    fn english_lines_fit_for_many_seeds() {
        use crate::i18n::{Lang, with_lang};
        with_lang(Lang::En, || {
            for seed in 0..100 {
                let doc = build(None, 4, Some(seed));
                for op in &doc.ops {
                    if let Op::Line { text, style } = op {
                        assert!(text.chars().count() <= style.columns(), "{text}");
                    }
                }
            }
            let doc = build(Some(Theme::Animaux), 1, Some(5));
            let texts: Vec<String> = doc
                .ops
                .iter()
                .filter_map(|op| match op {
                    Op::Line { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect();
            assert!(texts.iter().any(|t| t.contains("Answer: ")), "{texts:?}");
            assert!(texts.iter().any(|t| t.contains("MYSTERY WORD")), "{texts:?}");
        });
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
