//! Message codé : une phrase chiffrée (César, morse ou nombres) avec de quoi la déchiffrer,
//! et la réponse imprimée à l'envers en bas.

use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};
use serde::Deserialize;

use crate::doc::{Doc, Style};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cipher {
    /// Chaque lettre décalée dans l'alphabet.
    #[default]
    #[serde(alias = "césar")]
    Cesar,
    Morse,
    /// A = 1, B = 2…
    #[serde(alias = "numbers")]
    Nombres,
}

impl Cipher {
    pub fn label(self) -> &'static str {
        match self {
            Cipher::Cesar => "César",
            Cipher::Morse => "morse",
            Cipher::Nombres => "nombres",
        }
    }
}

/// Messages tirés au hasard quand le ticket n'en donne pas.
const MESSAGES: &[&str] = &[
    "Le trésor est caché sous le canapé",
    "Ce soir, c'est pizza",
    "Rendez-vous dans la cuisine à huit heures",
    "Le chat a mangé mes devoirs",
    "Tu es un agent secret de première classe",
    "Le mot de passe est banane",
    "Regarde derrière la porte du salon",
    "Celui qui lit ceci doit faire un câlin",
    "Il reste du chocolat dans le placard",
    "Mission accomplie, rentre à la base",
    "Les extraterrestres arrivent mardi",
    "Ne dis rien à personne",
    "Va voir dans la boîte aux lettres",
    "Demain, on va au parc",
];

const MORSE: [&str; 26] = [
    ".-", "-...", "-.-.", "-..", ".", "..-.", "--.", "....", "..", ".---", "-.-", ".-..", "--", "-.", "---", ".--.",
    "--.-", ".-.", "...", "-", "..-", "...-", ".--", "-..-", "-.--", "--..",
];

/// Majuscules sans accents ; tout ce qui n'est pas une lettre devient un espace.
fn normalize(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let form = super::word_search::grid_form(&c.to_string());
        if form.is_empty() {
            out.push(' ');
        } else {
            out.push_str(&form);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn letters(word: &str) -> impl Iterator<Item = usize> + '_ {
    word.bytes().map(|b| (b - b'A') as usize)
}

fn encode(text: &str, cipher: Cipher, shift: u8) -> String {
    let words = text.split(' ');
    match cipher {
        Cipher::Cesar => words
            .map(|w| letters(w).map(|i| (b'A' + ((i + shift as usize) % 26) as u8) as char).collect::<String>())
            .collect::<Vec<_>>()
            .join(" "),
        Cipher::Morse => words
            .map(|w| letters(w).map(|i| MORSE[i]).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join(" / "),
        Cipher::Nombres => words
            .map(|w| letters(w).map(|i| (i + 1).to_string()).collect::<Vec<_>>().join("-"))
            .collect::<Vec<_>>()
            .join(" / "),
    }
}

/// Lignes d'aide au déchiffrement, en petit (56 colonnes).
fn key(cipher: Cipher, shift: u8) -> Vec<String> {
    let alphabet: Vec<char> = ('A'..='Z').collect();
    match cipher {
        Cipher::Cesar => {
            let shifted: String = alphabet.iter().map(|&c| (b'A' + (c as u8 - b'A' + shift) % 26) as char).collect();
            vec![
                format!("Chaque lettre a avancé de {shift} : A est devenu {}.", &shifted[..1]),
                "En haut la lettre codée, en dessous la vraie lettre :".to_owned(),
                String::new(),
                shifted.chars().map(String::from).collect::<Vec<_>>().join(" "),
                alphabet.iter().map(char::to_string).collect::<Vec<_>>().join(" "),
            ]
        }
        Cipher::Morse => {
            let cells: Vec<String> = alphabet.iter().zip(MORSE).map(|(c, m)| format!("{c} {m:<6}")).collect();
            let mut lines = vec!["Un espace entre les lettres, « / » entre les mots.".to_owned(), String::new()];
            lines.extend(cells.chunks(6).map(|row| row.join(" ").trim_end().to_owned()));
            lines
        }
        Cipher::Nombres => {
            let cells: Vec<String> = alphabet.iter().enumerate().map(|(i, c)| format!("{c}={:<2}", i + 1)).collect();
            let mut lines = vec!["Chaque nombre est le rang d'une lettre dans l'alphabet.".to_owned(), String::new()];
            lines.extend(cells.chunks(9).map(|row| row.join(" ").trim_end().to_owned()));
            lines
        }
    }
}

pub fn build(message: Option<&str>, cipher: Cipher, shift: Option<u8>, answer: bool, seed: Option<u64>) -> Doc {
    let mut rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    let plain = message.filter(|m| !m.trim().is_empty()).unwrap_or_else(|| MESSAGES.choose(&mut rng).expect("messages"));
    let shift = shift.map(|s| s % 26).filter(|&s| s != 0).unwrap_or_else(|| rng.random_range(1..26));
    let coded = encode(&normalize(plain), cipher, shift);

    let mut doc = Doc::new();
    doc.header("Message codé");
    doc.text(&format!("Code : {}", cipher.label()), Style::default().small().center());
    doc.feed(1);
    let style = if cipher == Cipher::Cesar { Style::default().bold().size(2) } else { Style::default().bold() };
    doc.text(&coded, style.center());
    doc.feed(1);
    for line in key(cipher, shift) {
        doc.line(&line, Style::default().small());
    }
    if answer {
        doc.feed(1);
        doc.text(&format!("Message : {plain}"), Style::default().small().upside_down());
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Op;

    #[test]
    fn encodes_each_cipher() {
        assert_eq!(normalize("Ce soir, c'est pizza !"), "CE SOIR C EST PIZZA");
        assert_eq!(encode("ABC XYZ", Cipher::Cesar, 3), "DEF ABC");
        assert_eq!(encode("SOS", Cipher::Morse, 0), "... --- ...");
        assert_eq!(encode("AB C", Cipher::Nombres, 0), "1-2 / 3");
    }

    #[test]
    fn lines_fit() {
        for cipher in [Cipher::Cesar, Cipher::Morse, Cipher::Nombres] {
            for seed in 0..30 {
                let doc = build(None, cipher, None, true, Some(seed));
                for op in &doc.ops {
                    if let Op::Line { text, style } = op {
                        assert!(text.chars().count() <= style.columns(), "{text}");
                    }
                }
            }
        }
    }
}
