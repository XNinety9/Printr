//! Voie ferrée (« Tracks ») : relier A à B par une seule voie, en respectant le nombre de cases
//! de voie de chaque ligne et de chaque colonne. Grille générée localement, à solution unique.
//! Le numéro imprimé est la graine : le même numéro avec `solution: true` imprime la solution.

use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};

use super::sudoku::Difficulty;
use crate::doc::{Align, Doc, Style};
use crate::draw;

// Connexions d'une case, en masque de bits.
const UP: u8 = 1;
const RIGHT: u8 = 2;
const DOWN: u8 = 4;
const LEFT: u8 = 8;
const DIRS: [u8; 4] = [UP, RIGHT, DOWN, LEFT];

fn opposite(d: u8) -> u8 {
    match d {
        UP => DOWN,
        DOWN => UP,
        RIGHT => LEFT,
        _ => RIGHT,
    }
}

/// Case voisine dans la direction `d`, si elle est dans la grille.
fn step(n: usize, cell: usize, d: u8) -> Option<usize> {
    let (r, c) = (cell / n, cell % n);
    match d {
        UP if r > 0 => Some(cell - n),
        DOWN if r + 1 < n => Some(cell + n),
        LEFT if c > 0 => Some(cell - 1),
        RIGHT if c + 1 < n => Some(cell + 1),
        _ => None,
    }
}

/// (côté de la grille, pièces données en plus du minimum pour l'unicité)
fn settings(difficulty: Difficulty) -> (usize, usize) {
    match difficulty {
        Difficulty::Easy => (6, 3),
        Difficulty::Medium => (8, 1),
        Difficulty::Hard => (10, 0),
    }
}

pub struct Puzzle {
    pub n: usize,
    /// Ligne d'entrée A, par le bord gauche.
    pub entry: usize,
    /// Colonne de sortie B, par le bord bas.
    pub exit: usize,
    /// Pièce de chaque case dans la solution (0 : pas de voie).
    pub solution: Vec<u8>,
    /// Pièces imprimées d'office.
    pub given: Vec<u8>,
    pub rows: Vec<usize>,
    pub cols: Vec<usize>,
}

/// Chemin aléatoire sans croisement, de la colonne 0 jusqu'à la dernière ligne,
/// d'une longueur comprise entre `min` et `max` cases.
fn random_path(n: usize, rng: &mut StdRng) -> Vec<usize> {
    fn walk(
        n: usize,
        path: &mut Vec<usize>,
        seen: &mut [bool],
        (min, max): (usize, usize),
        budget: &mut u32,
        rng: &mut StdRng,
    ) -> bool {
        let cur = *path.last().expect("chemin non vide");
        if path.len() >= min && cur / n == n - 1 {
            return true;
        }
        if path.len() >= max || *budget == 0 {
            return false;
        }
        *budget -= 1;
        let mut dirs = DIRS;
        dirs.shuffle(rng);
        for d in dirs {
            if let Some(next) = step(n, cur, d).filter(|&c| !seen[c]) {
                seen[next] = true;
                path.push(next);
                if walk(n, path, seen, (min, max), budget, rng) {
                    return true;
                }
                path.pop();
                seen[next] = false;
            }
        }
        false
    }

    let bounds = (n * n * 2 / 5, n * n * 3 / 5);
    loop {
        let start = rng.random_range(0..n) * n;
        let mut path = vec![start];
        let mut seen = vec![false; n * n];
        seen[start] = true;
        if walk(n, &mut path, &mut seen, bounds, &mut 20_000, rng) {
            return path;
        }
    }
}

/// Pièces de chaque case d'un chemin entrant par la gauche et sortant par le bas.
fn pieces(n: usize, path: &[usize]) -> Vec<u8> {
    let mut masks = vec![0u8; n * n];
    masks[path[0]] |= LEFT;
    masks[*path.last().expect("chemin non vide")] |= DOWN;
    for pair in path.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let d = DIRS.into_iter().find(|&d| step(n, a, d) == Some(b)).expect("cases voisines");
        masks[a] |= d;
        masks[b] |= opposite(d);
    }
    masks
}

/// Recherche des solutions, de A vers B, avec un plafond de nœuds explorés.
struct Solver<'a> {
    n: usize,
    given: &'a [u8],
    exit: usize,
    rows: Vec<usize>,
    cols: Vec<usize>,
    left: usize,
    givens_left: usize,
    masks: Vec<u8>,
    nodes: u32,
    aborted: bool,
    found: Vec<Vec<u8>>,
}

const NODE_LIMIT: u32 = 300_000;

impl Solver<'_> {
    fn go(&mut self, cur: usize, from: u8) -> bool {
        self.nodes += 1;
        if self.nodes > NODE_LIMIT {
            self.aborted = true;
            return true;
        }
        let n = self.n;
        let dirs: Vec<u8> = match self.given[cur] {
            0 => DIRS.into_iter().filter(|&d| d != from).collect(),
            g => vec![g & !from],
        };
        for d in dirs {
            self.masks[cur] = from | d;
            if cur == self.exit && d == DOWN {
                if self.left == 0 && self.givens_left == 0 {
                    self.found.push(self.masks.clone());
                    if self.found.len() >= 2 {
                        return true;
                    }
                }
                continue;
            }
            let Some(next) = step(n, cur, d) else { continue };
            let (r, c) = (next / n, next % n);
            let back = opposite(d);
            if self.masks[next] != 0 || self.rows[r] == 0 || self.cols[c] == 0 {
                continue;
            }
            if self.given[next] != 0 && self.given[next] & back == 0 {
                continue;
            }
            // Il reste au moins la distance jusqu'à B à parcourir.
            let distance = (n - 1 - r) + c.abs_diff(self.exit % n);
            if self.left - 1 < distance {
                continue;
            }
            self.rows[r] -= 1;
            self.cols[c] -= 1;
            self.left -= 1;
            let is_given = self.given[next] != 0;
            if is_given {
                self.givens_left -= 1;
            }
            self.masks[next] = back; // marque la case comme occupée
            if self.go(next, back) {
                return true;
            }
            self.masks[next] = 0;
            if is_given {
                self.givens_left += 1;
            }
            self.rows[r] += 1;
            self.cols[c] += 1;
            self.left += 1;
        }
        self.masks[cur] = 0;
        false
    }
}

/// Jusqu'à deux solutions ; `None` si la recherche a dépassé son plafond.
fn solve(p: &Puzzle, given: &[u8]) -> Option<Vec<Vec<u8>>> {
    let n = p.n;
    let start = p.entry * n;
    let mut s = Solver {
        n,
        given,
        exit: (n - 1) * n + p.exit,
        rows: p.rows.clone(),
        cols: p.cols.clone(),
        left: p.rows.iter().sum::<usize>() - 1,
        givens_left: given.iter().filter(|&&g| g != 0).count(),
        masks: vec![0; n * n],
        nodes: 0,
        aborted: false,
        found: Vec::new(),
    };
    if given[start] != 0 && given[start] & LEFT == 0 {
        return Some(Vec::new());
    }
    if given[start] != 0 {
        s.givens_left -= 1;
    }
    s.rows[p.entry] -= 1;
    s.cols[0] -= 1;
    s.masks[start] = LEFT;
    s.go(start, LEFT);
    (!s.aborted).then_some(s.found)
}

pub fn generate(seed: u64, difficulty: Difficulty) -> Puzzle {
    let mut rng = StdRng::seed_from_u64(seed);
    let (n, extra) = settings(difficulty);
    let path = random_path(n, &mut rng);
    let solution = pieces(n, &path);
    let count = |cells: &mut dyn Iterator<Item = usize>| cells.filter(|&i| solution[i] != 0).count();
    let rows = (0..n).map(|r| count(&mut (r * n..(r + 1) * n))).collect();
    let cols = (0..n).map(|c| count(&mut (c..n * n).step_by(n))).collect();
    let mut p = Puzzle {
        n,
        entry: path[0] / n,
        exit: path.last().expect("chemin non vide") % n,
        given: vec![0; n * n],
        solution,
        rows,
        cols,
    };

    // On dévoile des pièces jusqu'à ce que la solution soit unique : de préférence une case
    // où la seconde solution trouvée diffère, sinon une case de la voie au hasard.
    loop {
        let hidden: Vec<usize> = path.iter().copied().filter(|&i| p.given[i] == 0).collect();
        let pick = match solve(&p, &p.given) {
            Some(found) if found.len() <= 1 => break,
            Some(found) => {
                let other = found.iter().find(|s| **s != p.solution).expect("deux solutions distinctes");
                let differ: Vec<usize> = hidden.iter().copied().filter(|&i| other[i] != p.solution[i]).collect();
                differ.choose(&mut rng).or(hidden.choose(&mut rng)).copied()
            }
            None => hidden.choose(&mut rng).copied(),
        };
        match pick {
            Some(i) => p.given[i] = p.solution[i],
            None => break,
        }
    }
    // Quelques pièces de plus pour les niveaux faciles.
    let mut hidden: Vec<usize> = path.iter().copied().filter(|&i| p.given[i] == 0).collect();
    hidden.shuffle(&mut rng);
    for &i in hidden.iter().take(extra) {
        p.given[i] = p.solution[i];
    }
    p
}

const BOTTOM_GAP: i64 = 10;

/// Pièce de voie : un trait épais du centre de la case vers chacun de ses bords connectés.
fn draw_piece(img: &mut image::GrayImage, mask: u8, x: i64, y: i64, cell: i64) {
    let t = (cell / 7).max(4);
    let (cx, cy, half) = (x + cell / 2, y + cell / 2, cell / 2);
    if mask & UP != 0 {
        draw::fill_rect(img, cx - t / 2, y, t, half + t / 2);
    }
    if mask & DOWN != 0 {
        draw::fill_rect(img, cx - t / 2, cy - t / 2, t, cell - half + t / 2);
    }
    if mask & LEFT != 0 {
        draw::fill_rect(img, x, cy - t / 2, half + t / 2, t);
    }
    if mask & RIGHT != 0 {
        draw::fill_rect(img, cx - t / 2, cy - t / 2, cell - half + t / 2, t);
    }
}

fn render(p: &Puzzle, solution: bool) -> image::GrayImage {
    let n = p.n as i64;
    // Une colonne de marge à gauche (A) et une à droite (nombres des lignes).
    let cell = ((512 - 16) / (n + 2)).min(52);
    let scale = ((cell - 10) / draw::DIGIT_H).min(4);
    let letter_scale = 3;
    let top = draw::DIGIT_H * scale + 12;
    let (ox, oy) = ((512 - (n + 2) * cell) / 2 + cell, top);
    let size = n * cell;
    let stub = cell / 3;
    let bottom = stub + 6 + draw::DIGIT_H * letter_scale + BOTTOM_GAP;
    let mut img = draw::canvas((oy + size + bottom) as u32);

    // Quadrillage fin, cadre épais.
    for k in 0..=n {
        let w = if k == 0 || k == n { 4 } else { 2 };
        draw::fill_rect(&mut img, ox + k * cell - w / 2, oy - 2, w, size + 4);
        draw::fill_rect(&mut img, ox - 2, oy + k * cell - w / 2, size + 4, w);
    }

    // Nombres : colonnes au-dessus, lignes à droite.
    // « 10 » s'imprime un cran plus petit, pour tenir dans la largeur d'une case.
    let fit = |v: usize| if v >= 10 { scale - 1 } else { scale };
    for (c, &v) in p.cols.iter().enumerate() {
        let y = oy - 8 - draw::DIGIT_H * fit(v);
        let cx = ox + c as i64 * cell + cell / 2;
        draw::number(&mut img, v as u32, cx - draw::number_width(v as u32, fit(v)) / 2, y, fit(v));
    }
    for (r, &v) in p.rows.iter().enumerate() {
        let y = oy + r as i64 * cell + (cell - draw::DIGIT_H * fit(v)) / 2;
        let cx = ox + size + cell / 2 + 2;
        draw::number(&mut img, v as u32, cx - draw::number_width(v as u32, fit(v)) / 2, y, fit(v));
    }

    // Entrée A par la gauche, sortie B par le bas.
    let t = (cell / 7).max(4);
    let ay = oy + p.entry as i64 * cell + cell / 2;
    draw::fill_rect(&mut img, ox - stub, ay - t / 2, stub, t);
    draw::letter(
        &mut img,
        b'A',
        ox - cell + 2,
        ay - draw::DIGIT_H * letter_scale / 2,
        letter_scale,
        draw::BLACK,
    );
    let bx = ox + p.exit as i64 * cell + cell / 2;
    draw::fill_rect(&mut img, bx - t / 2, oy + size, t, stub);
    draw::letter(
        &mut img,
        b'B',
        bx - draw::DIGIT_W * letter_scale / 2,
        oy + size + stub + 6,
        letter_scale,
        draw::BLACK,
    );

    let shown = if solution { &p.solution } else { &p.given };
    for (i, &mask) in shown.iter().enumerate() {
        if mask != 0 {
            let (x, y) = (ox + (i as i64 % n) * cell, oy + (i as i64 / n) * cell);
            draw_piece(&mut img, mask, x, y, cell);
        }
    }
    img
}

pub fn build(difficulty: Difficulty, seed: Option<u64>, solution: bool) -> Doc {
    let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
    let puzzle = generate(seed, difficulty);

    let mut doc = Doc::new();
    let title = if solution { "Solution de la voie ferrée" } else { "Voie ferrée" };
    doc.header(&format!("{title} · {}", difficulty.label()));
    if !solution {
        let small = Style::default().small().center();
        doc.text("Relie A à B par une seule voie, sans croisement.", small);
        doc.text("Nombres : cases de voie par ligne et par colonne.", small);
    }
    doc.feed(1);
    doc.image(render(&puzzle, solution));
    doc.text(&format!("n° {seed}"), Style::default().small().align(Align::Right));
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_and_clues_are_consistent() {
        for (seed, diff) in [(1, Difficulty::Easy), (42, Difficulty::Medium), (7, Difficulty::Hard)] {
            let p = generate(seed, diff);
            let track = p.solution.iter().filter(|&&m| m != 0).count();
            assert_eq!(p.rows.iter().sum::<usize>(), track);
            assert_eq!(p.cols.iter().sum::<usize>(), track);
            // Chaque case de voie a exactement deux connexions.
            assert!(p.solution.iter().all(|&m| m == 0 || m.count_ones() == 2));
            assert_eq!(p.solution[p.entry * p.n] & LEFT, LEFT);
            assert_eq!(p.solution[(p.n - 1) * p.n + p.exit] & DOWN, DOWN);
            // Les pièces données font partie de la solution.
            assert!(p.given.iter().zip(&p.solution).all(|(&g, &s)| g == 0 || g == s));
        }
    }

    #[test]
    fn solution_is_unique() {
        for seed in 1..=4 {
            for diff in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard] {
                let p = generate(seed, diff);
                let found = solve(&p, &p.given).expect("recherche terminée");
                assert_eq!(found, vec![p.solution.clone()], "graine {seed}, {diff:?}");
            }
        }
    }

    #[test]
    fn same_seed_same_grid() {
        let (a, b) = (generate(99, Difficulty::Medium), generate(99, Difficulty::Medium));
        assert_eq!(a.solution, b.solution);
        assert_eq!(a.given, b.given);
    }
}
