//! Calcul mental : une fiche d'opérations selon le niveau, les résultats imprimés à l'envers en bas.
//! Le numéro imprimé est la graine : le même numéro redonne la même fiche.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use super::sudoku::Difficulty;
use crate::doc::{Align, Doc, Style};

/// Une opération et son résultat.
struct Problem {
    text: String,
    answer: i64,
}

fn problem(difficulty: Difficulty, rng: &mut StdRng) -> Problem {
    let kind = rng.random_range(0..4);
    let (text, answer) = match (difficulty, kind) {
        // Facile : additions et soustractions jusqu'à 20, tables de 2 à 5.
        (Difficulty::Easy, 0) => {
            let (a, b) = (rng.random_range(1..=10), rng.random_range(1..=10));
            (format!("{a} + {b}"), a + b)
        }
        (Difficulty::Easy, 1) => {
            let (a, b) = (rng.random_range(5..=20), rng.random_range(1..=5));
            (format!("{a} - {b}"), a - b)
        }
        (Difficulty::Easy, _) => {
            let (a, b) = (rng.random_range(2..=5), rng.random_range(1..=10));
            (format!("{a} × {b}"), a * b)
        }
        // Moyen : deux chiffres avec retenue, toutes les tables, divisions exactes.
        (Difficulty::Medium, 0) => {
            let (a, b) = (rng.random_range(15..=79), rng.random_range(15..=49));
            (format!("{a} + {b}"), a + b)
        }
        (Difficulty::Medium, 1) => {
            let (a, b) = (rng.random_range(40..=99), rng.random_range(11..=39));
            (format!("{a} - {b}"), a - b)
        }
        (Difficulty::Medium, 2) => {
            let (a, b) = (rng.random_range(3..=9), rng.random_range(3..=10));
            (format!("{a} × {b}"), a * b)
        }
        (Difficulty::Medium, _) => {
            let (q, b) = (rng.random_range(2..=10), rng.random_range(2..=9));
            (format!("{} ÷ {b}", q * b), q)
        }
        // Difficile : trois chiffres, multiplications à deux chiffres, priorités.
        (Difficulty::Hard, 0) => {
            let (a, b) = (rng.random_range(120..=599), rng.random_range(85..=399));
            (format!("{a} + {b}"), a + b)
        }
        (Difficulty::Hard, 1) => {
            let (a, b) = (rng.random_range(12..=25), rng.random_range(3..=9));
            (format!("{a} × {b}"), a * b)
        }
        (Difficulty::Hard, 2) => {
            let (q, b) = (rng.random_range(11..=25), rng.random_range(3..=9));
            (format!("{} ÷ {b}", q * b), q)
        }
        (Difficulty::Hard, _) => {
            let (a, b, c) = (rng.random_range(2..=20), rng.random_range(2..=9), rng.random_range(2..=9));
            (format!("{a} + {b} × {c}"), a + b * c)
        }
    };
    Problem { text, answer }
}

fn title(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Easy => "Calcul mental · facile",
        Difficulty::Medium => "Calcul mental · moyen",
        Difficulty::Hard => "Calcul mental · difficile",
    }
}

pub fn build(difficulty: Difficulty, count: u8, seed: Option<u64>) -> Doc {
    let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
    let mut rng = StdRng::seed_from_u64(seed);
    let count = count.clamp(4, 30) as usize;
    let mut problems: Vec<Problem> = Vec::with_capacity(count);
    while problems.len() < count {
        let p = problem(difficulty, &mut rng);
        if !problems.iter().any(|q| q.text == p.text) {
            problems.push(p);
        }
    }

    let mut doc = Doc::new();
    doc.header(title(difficulty));
    doc.feed(1);
    // Deux colonnes de 21 caractères : « 12) 48 ÷ 6 = ____ ».
    let cell = |i: usize, p: &Problem| format!("{:>2}) {} = ____", i + 1, p.text);
    for (row, pair) in problems.chunks(2).enumerate() {
        let left = cell(row * 2, &pair[0]);
        let right = pair.get(1).map(|p| cell(row * 2 + 1, p)).unwrap_or_default();
        doc.line(&format!("{left:<21}{right}"), Style::default());
        doc.feed(1);
    }
    doc.line(&format!("{:>42}", "Score : ...... / ".to_owned() + &count.to_string()), Style::default().bold());
    doc.text(&format!("n° {seed}"), Style::default().small().align(Align::Right));
    doc.feed(1);
    let answers: Vec<String> = problems.iter().enumerate().map(|(i, p)| format!("{}) {}", i + 1, p.answer)).collect();
    doc.text(&format!("Réponses : {}", answers.join("  ")), Style::default().small().upside_down());
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Op;

    #[test]
    fn answers_are_right_and_lines_fit() {
        for difficulty in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard] {
            let mut rng = StdRng::seed_from_u64(3);
            for _ in 0..500 {
                let p = problem(difficulty, &mut rng);
                let expr = p.text.replace('×', "*").replace('÷', "/");
                assert_eq!(eval(&expr), p.answer, "{}", p.text);
                assert!(p.answer >= 0);
                assert!(format!("30) {} = ____", p.text).chars().count() <= 21, "{}", p.text);
            }
            let doc = build(difficulty, 30, Some(9));
            for op in &doc.ops {
                if let Op::Line { text, style } = op {
                    assert!(text.chars().count() <= style.columns(), "{text}");
                }
            }
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
