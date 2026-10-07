//! Petit défi de logique quotidien, déterministe et disponible hors ligne.

use chrono::{Datelike, NaiveDate};

use crate::doc::{Doc, Style};
use crate::fr;

const CHALLENGES: &[(&str, &str)] = &[
    (
        "Quelle est la suite ? 1, 4, 9, 16, …",
        "25 (les carrés de 1, 2, 3, 4, 5).",
    ),
    (
        "Quel nombre complète la suite ? 2, 6, 12, 20, 30, …",
        "42 (n × (n + 1)).",
    ),
    (
        "Je peux être tenue ou rompue, mais pas touchée. Qui suis-je ?",
        "Une promesse.",
    ),
    (
        "Plus on m'enlève, plus je grandis. Qui suis-je ?",
        "Un trou.",
    ),
    ("Combien de mois ont au moins 28 jours ?", "Les 12 mois."),
    (
        "Un père a 4 filles. Chaque fille a le même frère. Combien d'enfants ?",
        "5 enfants.",
    ),
    (
        "5 machines fabriquent 5 pièces en 5 minutes. 100 machines en font 100 en combien de temps ?",
        "5 minutes.",
    ),
    (
        "Quel est le prochain jour après mardi, dans 10 jours ?",
        "Vendredi.",
    ),
    (
        "Je monte chaque année, mais ne redescends jamais. Qui suis-je ?",
        "Ton âge.",
    ),
    (
        "Quel nombre pair est supérieur à 20, inférieur à 30 et divisible par 3 ?",
        "24.",
    ),
    (
        "Un livre et un marque-page coûtent 11 €. Le livre coûte 10 € de plus. Prix du marque-page ?",
        "0,50 € (et le livre 10,50 €).",
    ),
    (
        "Combien de fois peut-on soustraire 10 de 100 ?",
        "Une fois ; ensuite on le soustrait de 90.",
    ),
    (
        "Deux pièces font 30 centimes. L'une n'est pas une pièce de 10 centimes. Quelles sont-elles ?",
        "Une pièce de 20 c et une de 10 c.",
    ),
    (
        "Quelle lettre vient ensuite ? J, F, M, A, M, J, …",
        "J (les initiales des mois).",
    ),
];

pub fn build(today: NaiveDate, show_answer: bool) -> Doc {
    let index = today.num_days_from_ce() as usize % CHALLENGES.len();
    let (question, answer) = CHALLENGES[index];
    let mut doc = Doc::new();
    doc.header("Défi du jour");
    doc.text(
        &fr::capitalize(&fr::long_date(today)),
        Style::default().small().center(),
    );
    doc.feed(1).hanging("- ", question, Style::default().bold());
    if show_answer {
        doc.feed(1)
            .hanging("Réponse : ", answer, Style::default().small());
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_is_hidden_unless_requested() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        let hidden = build(date, false).preview(false);
        let shown = build(date, true).preview(false);
        assert!(hidden.contains("DÉFI DU JOUR"));
        assert!(!hidden.contains("Réponse :"));
        assert!(shown.contains("Réponse :"));
    }
}
