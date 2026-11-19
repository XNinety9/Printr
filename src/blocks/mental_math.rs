//! Calcul mental : une fiche d'opérations selon le niveau, les résultats imprimés à l'envers en bas.
//! Le numéro imprimé est la graine : le même numéro redonne la même fiche.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::Deserialize;

use crate::doc::{Align, Doc, Style};

/// Niveau de la fiche. Mêmes noms que les autres jeux, plus « débutant » pour le début de
/// l'élémentaire (issue n° 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    #[serde(alias = "debutant", alias = "débutant")]
    Beginner,
    #[serde(alias = "facile")]
    Easy,
    #[default]
    #[serde(alias = "moyen")]
    Medium,
    #[serde(alias = "difficile")]
    Hard,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Beginner => crate::i18n::t("débutant", "beginner"),
            Level::Easy => crate::i18n::t("facile", "easy"),
            Level::Medium => crate::i18n::t("moyen", "medium"),
            Level::Hard => crate::i18n::t("difficile", "hard"),
        }
    }
}

/// Une opération à proposer sur la fiche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    #[serde(alias = "additions", alias = "+", alias = "plus")]
    Addition,
    #[serde(alias = "soustraction", alias = "soustractions", alias = "subtractions", alias = "-", alias = "−")]
    Subtraction,
    #[serde(alias = "multiplications", alias = "×", alias = "x", alias = "*", alias = "fois")]
    Multiplication,
    #[serde(alias = "divisions", alias = "÷", alias = "/")]
    Division,
}

impl Operation {
    const ALL: [Operation; 4] =
        [Operation::Addition, Operation::Subtraction, Operation::Multiplication, Operation::Division];

    pub fn symbol(self) -> &'static str {
        match self {
            Operation::Addition => "+",
            Operation::Subtraction => "−",
            Operation::Multiplication => "×",
            Operation::Division => "÷",
        }
    }
}

/// Ce que l'on demande à la fiche, en plus du niveau.
#[derive(Clone, Debug, Default)]
pub struct Choice {
    /// Opérations retenues ; vide : celles du niveau, comme avant l'issue n° 4.
    pub operations: Vec<Operation>,
    /// Tables à réviser pour les multiplications et divisions ; vide : celles du niveau.
    pub tables: Vec<u8>,
}

impl Choice {
    /// Sans choix particulier, la fiche reste celle d'avant (même numéro, mêmes opérations).
    fn is_default(&self) -> bool {
        self.operations.is_empty() && self.tables.is_empty()
    }

    fn tables(&self) -> Vec<i64> {
        self.tables.iter().filter(|t| (1..=12).contains(*t)).map(|&t| t as i64).collect()
    }
}

/// Une opération et son résultat.
struct Problem {
    text: String,
    answer: i64,
}

fn problem(level: Level, choice: &Choice, rng: &mut StdRng) -> Problem {
    if choice.is_default()
        && let Some(p) = classic(level, rng)
    {
        return p;
    }
    let operations = if choice.operations.is_empty() { Operation::ALL.to_vec() } else { choice.operations.clone() };
    let op = operations[rng.random_range(0..operations.len())];
    generate(level, op, &choice.tables(), rng)
}

/// Un nombre au hasard dans les tables choisies, sinon dans `range`.
fn table(tables: &[i64], rng: &mut StdRng, range: std::ops::RangeInclusive<i64>) -> i64 {
    if tables.is_empty() { rng.random_range(range) } else { tables[rng.random_range(0..tables.len())] }
}

/// Une opération d'un type donné, adaptée au niveau.
fn generate(level: Level, op: Operation, tables: &[i64], rng: &mut StdRng) -> Problem {
    use Operation::*;
    let (text, answer) = match (level, op) {
        // Débutant : sommes jusqu'à 10, soustractions sans résultat négatif ni retenue, doubles.
        (Level::Beginner, Addition) => {
            let a = rng.random_range(1..=9);
            let b = rng.random_range(1..=10 - a);
            (format!("{a} + {b}"), a + b)
        }
        (Level::Beginner, Subtraction) => {
            let a = rng.random_range(2..=10);
            let b = rng.random_range(1..a);
            (format!("{a} - {b}"), a - b)
        }
        (Level::Beginner, Multiplication) => {
            let (a, b) = (table(tables, rng, 2..=2), rng.random_range(1..=5));
            (format!("{a} × {b}"), a * b)
        }
        (Level::Beginner, Division) => {
            let (b, q) = (table(tables, rng, 2..=2), rng.random_range(1..=5));
            (format!("{} ÷ {b}", q * b), q)
        }
        // Facile : jusqu'à 20, tables de 2 à 5.
        (Level::Easy, Addition) => {
            let (a, b) = (rng.random_range(1..=10), rng.random_range(1..=10));
            (format!("{a} + {b}"), a + b)
        }
        (Level::Easy, Subtraction) => {
            let (a, b) = (rng.random_range(5..=20), rng.random_range(1..=5));
            (format!("{a} - {b}"), a - b)
        }
        (Level::Easy, Multiplication) => {
            let (a, b) = (table(tables, rng, 2..=5), rng.random_range(1..=10));
            (format!("{a} × {b}"), a * b)
        }
        (Level::Easy, Division) => {
            let (b, q) = (table(tables, rng, 2..=5), rng.random_range(1..=10));
            (format!("{} ÷ {b}", q * b), q)
        }
        // Moyen : deux chiffres avec retenue, toutes les tables, divisions exactes.
        (Level::Medium, Addition) => {
            let (a, b) = (rng.random_range(15..=79), rng.random_range(15..=49));
            (format!("{a} + {b}"), a + b)
        }
        (Level::Medium, Subtraction) => {
            let (a, b) = (rng.random_range(40..=99), rng.random_range(11..=39));
            (format!("{a} - {b}"), a - b)
        }
        (Level::Medium, Multiplication) => {
            let (a, b) = (table(tables, rng, 3..=9), rng.random_range(3..=10));
            (format!("{a} × {b}"), a * b)
        }
        (Level::Medium, Division) => {
            let (b, q) = (table(tables, rng, 2..=9), rng.random_range(2..=10));
            (format!("{} ÷ {b}", q * b), q)
        }
        // Difficile : trois chiffres, multiplications à deux chiffres.
        (Level::Hard, Addition) => {
            let (a, b) = (rng.random_range(120..=599), rng.random_range(85..=399));
            (format!("{a} + {b}"), a + b)
        }
        (Level::Hard, Subtraction) => {
            let (a, b) = (rng.random_range(300..=999), rng.random_range(85..=299));
            (format!("{a} - {b}"), a - b)
        }
        (Level::Hard, Multiplication) => {
            let (a, b) = (rng.random_range(12..=25), table(tables, rng, 3..=9));
            (format!("{a} × {b}"), a * b)
        }
        (Level::Hard, Division) => {
            let (q, b) = (rng.random_range(11..=25), table(tables, rng, 3..=9));
            (format!("{} ÷ {b}", q * b), q)
        }
    };
    Problem { text, answer }
}

/// La fiche d'avant l'issue n° 4, au tirage près : un même numéro redonne la même fiche.
fn classic(level: Level, rng: &mut StdRng) -> Option<Problem> {
    let kind = match level {
        Level::Beginner => return None,
        _ => rng.random_range(0..4),
    };
    let (text, answer) = match (level, kind) {
        // Facile : additions et soustractions jusqu'à 20, tables de 2 à 5.
        (Level::Easy, 0) => {
            let (a, b) = (rng.random_range(1..=10), rng.random_range(1..=10));
            (format!("{a} + {b}"), a + b)
        }
        (Level::Easy, 1) => {
            let (a, b) = (rng.random_range(5..=20), rng.random_range(1..=5));
            (format!("{a} - {b}"), a - b)
        }
        (Level::Easy, _) => {
            let (a, b) = (rng.random_range(2..=5), rng.random_range(1..=10));
            (format!("{a} × {b}"), a * b)
        }
        // Moyen : deux chiffres avec retenue, toutes les tables, divisions exactes.
        (Level::Medium, 0) => {
            let (a, b) = (rng.random_range(15..=79), rng.random_range(15..=49));
            (format!("{a} + {b}"), a + b)
        }
        (Level::Medium, 1) => {
            let (a, b) = (rng.random_range(40..=99), rng.random_range(11..=39));
            (format!("{a} - {b}"), a - b)
        }
        (Level::Medium, 2) => {
            let (a, b) = (rng.random_range(3..=9), rng.random_range(3..=10));
            (format!("{a} × {b}"), a * b)
        }
        (Level::Medium, _) => {
            let (q, b) = (rng.random_range(2..=10), rng.random_range(2..=9));
            (format!("{} ÷ {b}", q * b), q)
        }
        // Difficile : trois chiffres, multiplications à deux chiffres, priorités.
        (Level::Hard, 0) => {
            let (a, b) = (rng.random_range(120..=599), rng.random_range(85..=399));
            (format!("{a} + {b}"), a + b)
        }
        (Level::Hard, 1) => {
            let (a, b) = (rng.random_range(12..=25), rng.random_range(3..=9));
            (format!("{a} × {b}"), a * b)
        }
        (Level::Hard, 2) => {
            let (q, b) = (rng.random_range(11..=25), rng.random_range(3..=9));
            (format!("{} ÷ {b}", q * b), q)
        }
        (Level::Hard, _) => {
            let (a, b, c) = (rng.random_range(2..=20), rng.random_range(2..=9), rng.random_range(2..=9));
            (format!("{a} + {b} × {c}"), a + b * c)
        }
        (Level::Beginner, _) => unreachable!(),
    };
    Some(Problem { text, answer })
}

fn title(level: Level) -> String {
    tr!("Calcul mental · {}", "Mental math · {}", level.label())
}

pub fn build(level: Level, choice: &Choice, count: u8, seed: Option<u64>) -> Doc {
    let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
    let mut rng = StdRng::seed_from_u64(seed);
    let count = count.clamp(4, 30) as usize;
    let mut problems: Vec<Problem> = Vec::with_capacity(count);
    // Pas deux fois la même opération… tant que c'est possible : avec une seule table au niveau
    // débutant, il n'existe que cinq opérations différentes.
    let mut tries = 0;
    while problems.len() < count {
        let p = problem(level, choice, &mut rng);
        tries += 1;
        if tries > 1000 || !problems.iter().any(|q| q.text == p.text) {
            problems.push(p);
        }
    }

    let mut doc = Doc::new();
    doc.header(&title(level));
    doc.feed(1);
    // Deux colonnes d'une demi-largeur (21 caractères en 512 points) : « 12) 48 ÷ 6 = ____ ».
    let width = Style::default().columns();
    let half = width / 2;
    let cell = |i: usize, p: &Problem| format!("{:>2}) {} = ____", i + 1, p.text);
    for (row, pair) in problems.chunks(2).enumerate() {
        let left = cell(row * 2, &pair[0]);
        let right = pair.get(1).map(|p| cell(row * 2 + 1, p)).unwrap_or_default();
        doc.line(&format!("{left:<half$}{right}"), Style::default());
        doc.feed(1);
    }
    let score = tr!("Score : ...... / {count}", "Score: ...... / {count}");
    doc.line(&format!("{score:>width$}"), Style::default().bold());
    doc.text(&tr!("n° {seed}", "No. {seed}"), Style::default().small().align(Align::Right));
    doc.feed(1);
    let answers: Vec<String> = problems.iter().enumerate().map(|(i, p)| format!("{}) {}", i + 1, p.answer)).collect();
    let answers = answers.join("  ");
    doc.text(&tr!("Réponses : {answers}", "Answers: {answers}"), Style::default().small().upside_down());
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Op;

    #[test]
    fn answers_are_right_and_lines_fit() {
        let choices = [
            Choice::default(),
            Choice { operations: vec![Operation::Addition, Operation::Subtraction], tables: vec![] },
            Choice { operations: vec![Operation::Multiplication, Operation::Division], tables: vec![7, 8] },
            Choice { operations: vec![Operation::Subtraction], tables: vec![] },
        ];
        for difficulty in [Level::Beginner, Level::Easy, Level::Medium, Level::Hard] {
            for choice in &choices {
                let mut rng = StdRng::seed_from_u64(3);
                for _ in 0..500 {
                    let p = problem(difficulty, choice, &mut rng);
                    let expr = p.text.replace('×', "*").replace('÷', "/");
                    assert_eq!(eval(&expr), p.answer, "{}", p.text);
                    assert!(p.answer >= 0);
                    assert!(format!("30) {} = ____", p.text).chars().count() <= 21, "{}", p.text);
                }
                let doc = build(difficulty, choice, 30, Some(9));
                for op in &doc.ops {
                    if let Op::Line { text, style } = op {
                        assert!(text.chars().count() <= style.columns(), "{text}");
                    }
                }
            }
        }
    }

    /// Sans choix particulier, la fiche est exactement celle d'avant l'issue n° 4.
    #[test]
    fn default_sheets_are_unchanged() {
        let texts = |doc: &Doc| -> Vec<String> {
            doc.ops
                .iter()
                .filter_map(|op| if let Op::Line { text, .. } = op { Some(text.clone()) } else { None })
                .collect()
        };
        let lines = texts(&build(Level::Easy, &Choice::default(), 10, Some(2026)));
        // Même fiche que la 1.6 (vérifié contre son binaire sur 120 fiches).
        assert_eq!(lines[1], " 1) 4 + 8 = ____      2) 4 × 7 = ____");
    }

    /// Le choix des opérations et des tables est respecté (issue n° 4).
    #[test]
    fn chosen_operations_and_tables_only() {
        let mut rng = StdRng::seed_from_u64(1);
        let only_add_sub = Choice { operations: vec![Operation::Addition, Operation::Subtraction], tables: vec![] };
        for _ in 0..300 {
            let p = problem(Level::Beginner, &only_add_sub, &mut rng);
            assert!(!p.text.contains('×') && !p.text.contains('÷'), "{}", p.text);
            assert!(p.answer <= 10, "{}", p.text);
        }
        let table_seven = Choice { operations: vec![Operation::Multiplication], tables: vec![7] };
        for _ in 0..100 {
            let p = problem(Level::Easy, &table_seven, &mut rng);
            assert!(p.text.starts_with("7 × "), "{}", p.text);
        }
    }

    /// Évalue « a op b » ou « a + b * c », avec les priorités.
    fn eval(expr: &str) -> i64 {
        let tokens: Vec<&str> = expr.split(' ').collect();
        match tokens.as_slice() {
            [a, "+", b, "*", c] => a.parse::<i64>().unwrap() + b.parse::<i64>().unwrap() * c.parse::<i64>().unwrap(),
            [a, op, b] => {
                let (a, b) = (a.parse::<i64>().unwrap(), b.parse::<i64>().unwrap());
                match *op {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" => {
                        assert_eq!(a % b, 0, "division non exacte");
                        a / b
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!("{expr}"),
        }
    }
}
