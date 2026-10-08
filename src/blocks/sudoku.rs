//! Sudoku généré localement, à solution unique, imprimé en grille à remplir au stylo.
//! Le numéro imprimé est la graine : le même numéro avec `solution: true` imprime la solution.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};
use serde::Deserialize;

use crate::doc::{Doc, Style};
use crate::draw;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    #[serde(alias = "facile")]
    Easy,
    #[default]
    #[serde(alias = "moyen")]
    Medium,
    #[serde(alias = "difficile")]
    Hard,
}

impl Difficulty {
    fn clues(self) -> usize {
        match self {
            Difficulty::Easy => 38,
            Difficulty::Medium => 30,
            Difficulty::Hard => 25,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Easy => "facile",
            Difficulty::Medium => "moyen",
            Difficulty::Hard => "difficile",
        }
    }
}

type Grid = [u8; 81];

/// Chiffres encore possibles dans la case `i` (bit n = chiffre n).
fn candidates(g: &Grid, i: usize) -> u16 {
    let (r, c) = (i / 9, i % 9);
    let (br, bc) = (r / 3 * 3, c / 3 * 3);
    let mut used = 0u16;
    for k in 0..9 {
        used |= 1 << g[r * 9 + k];
        used |= 1 << g[k * 9 + c];
        used |= 1 << g[(br + k / 3) * 9 + bc + k % 3];
    }
    !used & 0b11_1111_1110
}

/// Case vide ayant le moins de candidats, ou `None` si la grille est pleine.
fn best_cell(g: &Grid) -> Option<(usize, u16)> {
    let mut best: Option<(usize, u16)> = None;
    for i in (0..81).filter(|&i| g[i] == 0) {
        let cand = candidates(g, i);
        if best.is_none_or(|(_, b)| cand.count_ones() < b.count_ones()) {
            best = Some((i, cand));
            if cand.count_ones() <= 1 {
                break;
            }
        }
    }
    best
}

/// Remplit la grille au hasard ; renvoie faux si impossible.
fn fill(g: &mut Grid, rng: &mut StdRng) -> bool {
    let Some((i, cand)) = best_cell(g) else { return true };
    let mut digits: Vec<u8> = (1..=9).filter(|d| cand & (1 << d) != 0).collect();
    digits.shuffle(rng);
    for d in digits {
        g[i] = d;
        if fill(g, rng) {
            return true;
        }
    }
    g[i] = 0;
    false
}

/// Nombre de solutions, plafonné à `limit`.
fn count_solutions(g: &mut Grid, limit: usize) -> usize {
    let Some((i, cand)) = best_cell(g) else { return 1 };
    let mut count = 0;
    for d in (1..=9).filter(|d| cand & (1 << d) != 0) {
        g[i] = d;
        count += count_solutions(g, limit - count);
        if count >= limit {
            break;
        }
    }
    g[i] = 0;
    count
}

/// (grille à résoudre, solution)
pub fn generate(seed: u64, difficulty: Difficulty) -> (Grid, Grid) {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut solution = [0; 81];
    fill(&mut solution, &mut rng);

    let mut puzzle = solution;
    let mut cells: Vec<usize> = (0..81).collect();
    cells.shuffle(&mut rng);
    let mut filled = 81;
    for i in cells {
        if filled <= difficulty.clues() {
            break;
        }
        puzzle[i] = 0;
        if count_solutions(&mut puzzle.clone(), 2) == 1 {
            filled -= 1;
        } else {
            puzzle[i] = solution[i];
        }
    }
    (puzzle, solution)
}

const CELL: i64 = 54;
const SCALE: i64 = 5;
/// Blanc sous la grille, pour décoller le numéro.
const BOTTOM_GAP: i64 = 10;

fn render(grid: &Grid) -> image::GrayImage {
    let size = CELL * 9;
    let (ox, oy) = ((512 - size) / 2, 3);
    let mut img = draw::canvas((size + 6 + BOTTOM_GAP) as u32);
    for k in 0..=9 {
        let thick = if k % 3 == 0 { 5 } else { 2 };
        let pos = k * CELL - thick / 2;
        draw::fill_rect(&mut img, ox + pos, oy - 2, thick, size + 4);
        draw::fill_rect(&mut img, ox - 2, oy + pos, size + 4, thick);
    }
    for (i, &d) in grid.iter().enumerate().filter(|(_, d)| **d != 0) {
        let (r, c) = ((i / 9) as i64, (i % 9) as i64);
        let x = ox + c * CELL + (CELL - draw::DIGIT_W * SCALE) / 2;
        let y = oy + r * CELL + (CELL - draw::DIGIT_H * SCALE) / 2;
        draw::digit(&mut img, d, x, y, SCALE);
    }
    img
}

pub fn build(difficulty: Difficulty, seed: Option<u64>, solution: bool) -> Doc {
    let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
    let (puzzle, solved) = generate(seed, difficulty);

    let mut doc = Doc::new();
    let title = if solution { "Solution du sudoku" } else { "Sudoku" };
    doc.header(&format!("{title} · {}", difficulty.label()));
    doc.feed(1);
    doc.image(render(if solution { &solved } else { &puzzle }));
    doc.text(&format!("n° {seed}"), Style::default().small().align(crate::doc::Align::Right));
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puzzle_has_unique_solution_matching_grid() {
        for (seed, diff) in [(1, Difficulty::Easy), (42, Difficulty::Medium), (7, Difficulty::Hard)] {
            let (puzzle, solution) = generate(seed, diff);
            assert!(solution.iter().all(|&d| (1..=9).contains(&d)));
            for i in 0..81 {
                let mut g = solution;
                let d = g[i];
                g[i] = 0;
                assert_eq!(candidates(&g, i), 1 << d, "grille invalide");
            }
            assert!(puzzle.iter().zip(&solution).all(|(p, s)| *p == 0 || p == s));
            assert_eq!(count_solutions(&mut puzzle.clone(), 2), 1);
            assert!(puzzle.iter().filter(|&&d| d != 0).count() >= diff.clues());
        }
    }

    #[test]
    fn same_seed_same_grid() {
        assert_eq!(generate(123, Difficulty::Medium), generate(123, Difficulty::Medium));
    }
}
