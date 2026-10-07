//! Primitives de dessin sur image noir et blanc (sudoku, labyrinthe).

use image::{GrayImage, Luma};

use crate::raster::PRINT_WIDTH;

pub const BLACK: Luma<u8> = Luma([0]);

/// Canevas blanc pleine largeur.
pub fn canvas(height: u32) -> GrayImage {
    GrayImage::from_pixel(PRINT_WIDTH, height, Luma([255]))
}

/// Rectangle plein, rogné aux bords de l'image.
pub fn fill_rect(img: &mut GrayImage, x: i64, y: i64, w: i64, h: i64) {
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    for py in y.max(0)..(y + h).min(ih) {
        for px in x.max(0)..(x + w).min(iw) {
            img.put_pixel(px as u32, py as u32, BLACK);
        }
    }
}

/// Chiffres 1 à 9 en 5×7.
const DIGITS: [[&str; 7]; 9] = [
    ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
    [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
    ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###."],
    ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
    ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
    ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."],
    ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
    [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
    [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."],
];

pub const DIGIT_W: i64 = 5;
pub const DIGIT_H: i64 = 7;

/// Dessine le chiffre `d` (1..=9) à l'échelle `scale`, coin haut-gauche en (x, y).
pub fn digit(img: &mut GrayImage, d: u8, x: i64, y: i64, scale: i64) {
    let glyph = &DIGITS[(d - 1) as usize];
    for (row, line) in glyph.iter().enumerate() {
        for (col, c) in line.bytes().enumerate() {
            if c == b'#' {
                fill_rect(img, x + col as i64 * scale, y + row as i64 * scale, scale, scale);
            }
        }
    }
}
