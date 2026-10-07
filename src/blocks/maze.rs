//! Labyrinthe parfait (un seul chemin entre deux cases), généré par parcours en profondeur.

use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};

use crate::doc::{Align, Doc, Style};
use crate::draw;

/// Murs à droite et en bas de chaque case.
struct Maze {
    w: usize,
    h: usize,
    right: Vec<bool>,
    bottom: Vec<bool>,
}

fn generate(w: usize, h: usize, seed: u64) -> Maze {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut maze = Maze { w, h, right: vec![true; w * h], bottom: vec![true; w * h] };
    let mut visited = vec![false; w * h];
    let mut stack = vec![0usize];
    visited[0] = true;
    while let Some(&cell) = stack.last() {
        let (x, y) = (cell % w, cell / w);
        let mut next = Vec::with_capacity(4);
        if x > 0 && !visited[cell - 1] {
            next.push(cell - 1);
        }
        if x + 1 < w && !visited[cell + 1] {
            next.push(cell + 1);
        }
        if y > 0 && !visited[cell - w] {
            next.push(cell - w);
        }
        if y + 1 < h && !visited[cell + w] {
            next.push(cell + w);
        }
        match next.choose(&mut rng) {
            Some(&n) => {
                // On abat le mur entre `cell` et `n`.
                match n {
                    n if n == cell + 1 => maze.right[cell] = false,
                    n if n + 1 == cell => maze.right[n] = false,
                    n if n == cell + w => maze.bottom[cell] = false,
                    _ => maze.bottom[n] = false,
                }
                visited[n] = true;
                stack.push(n);
            }
            None => {
                stack.pop();
            }
        }
    }
    maze
}

const WALL: i64 = 4;

fn render(m: &Maze) -> image::GrayImage {
    let cell = ((512 - 2 * WALL) / m.w as i64).min(48);
    let (gw, gh) = (cell * m.w as i64, cell * m.h as i64);
    let (ox, oy) = ((512 - gw) / 2, WALL);
    let mut img = draw::canvas((gh + 2 * WALL) as u32);
    let half = WALL / 2;

    // Bords : entrée en haut à gauche, sortie en bas à droite.
    draw::fill_rect(&mut img, ox + cell - half, oy - half, gw - cell + WALL, WALL);
    draw::fill_rect(&mut img, ox - half, oy + gh - half, gw - cell + WALL, WALL);
    draw::fill_rect(&mut img, ox - half, oy - half, WALL, gh + WALL);
    draw::fill_rect(&mut img, ox + gw - half, oy - half, WALL, gh + WALL);

    for y in 0..m.h {
        for x in 0..m.w {
            let i = y * m.w + x;
            let (px, py) = (ox + x as i64 * cell, oy + y as i64 * cell);
            if m.right[i] && x + 1 < m.w {
                draw::fill_rect(&mut img, px + cell - half, py - half, WALL, cell + WALL);
            }
            if m.bottom[i] && y + 1 < m.h {
                draw::fill_rect(&mut img, px - half, py + cell - half, cell + WALL, WALL);
            }
        }
    }
    img
}

pub fn build(width: u8, height: u8, seed: Option<u64>) -> Doc {
    let (w, h) = (width.clamp(4, 40) as usize, height.clamp(4, 60) as usize);
    let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));

    let mut doc = Doc::new();
    doc.header("Labyrinthe");
    doc.text("Entrée en haut, sortie en bas", Style::default().small().center());
    doc.feed(1);
    doc.image(render(&generate(w, h, seed)));
    doc.text(&format!("n° {seed}"), Style::default().small().align(Align::Right));
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_maze_is_a_spanning_tree() {
        let (w, h) = (12, 16);
        let m = generate(w, h, 99);
        let open = m.right.iter().chain(&m.bottom).filter(|&&wall| !wall).count();
        // Un arbre couvrant sur w*h cases a exactement w*h - 1 passages.
        assert_eq!(open, w * h - 1);
    }
}
