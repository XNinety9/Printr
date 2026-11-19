//! Logimage (picross) : noircir les cases d'après les indices pour révéler un dessin.
//! Chaque dessin a une solution unique, trouvable par simple déduction ligne à ligne (vérifié
//! par les tests). Le numéro imprimé est celui du dessin : `solution: true` imprime le dessin.

use image::GrayImage;
use rand::RngExt;

use crate::doc::{Align, Doc, Style};
use crate::draw;
use crate::raster::PRINT_WIDTH;

const N: usize = 10;

/// (nom, dessin de 10×10, `#` = case noire)
#[rustfmt::skip]
const PICTURES: &[(&str, [&str; N])] = &[
    ("cœur", [
        ".##....##.",
        "####..####",
        "##########",
        "##########",
        "##########",
        ".########.",
        "..######..",
        "...####...",
        "....##....",
        "..........",
    ]),
    ("sapin", [
        "....##....",
        "...####...",
        "..######..",
        "....##....",
        "..######..",
        ".########.",
        "...####...",
        "##########",
        "....##....",
        "...####...",
    ]),
    ("bateau", [
        "....#.....",
        "....##....",
        "....###...",
        "....####..",
        "....#####.",
        "....#.....",
        "##########",
        ".########.",
        "..######..",
        "~~~~~~~~~~",
    ]),
    ("sourire", [
        "..######..",
        ".########.",
        "##.####.##",
        "##.####.##",
        "##########",
        "#.######.#",
        "##.####.##",
        "###....###",
        ".########.",
        "..######..",
    ]),
    ("champignon", [
        "...####...",
        ".##.##.##.",
        "##########",
        "#.##..##.#",
        "##########",
        "...#..#...",
        "...#..#...",
        "...####...",
        "..........",
        "..........",
    ]),
    ("parapluie", [
        "....##....",
        "..######..",
        ".########.",
        "##########",
        "#.#.##.#.#",
        "....##....",
        "....##....",
        "....##....",
        ".#..##....",
        ".####.....",
    ]),
    ("maison", [
        "....##....",
        "...####...",
        "..######..",
        ".########.",
        "##########",
        ".#......#.",
        ".#.##.#.#.",
        ".#.##...#.",
        ".#.##...#.",
        ".########.",
    ]),
    ("chat", [
        "#...#.....",
        "##.##.....",
        "#####.....",
        "#.#.#.....",
        "#####....#",
        ".###.....#",
        "#####...#.",
        "######.##.",
        "#######...",
        "##.##.##..",
    ]),
    ("poisson", [
        "..........",
        "...####...",
        ".#######.#",
        "##.######.",
        "#########.",
        ".#######.#",
        "...####...",
        "....#.....",
        "..........",
        "..........",
    ]),
    ("tasse", [
        "...#.#....",
        "....#.#...",
        "...#.#....",
        "..........",
        "########..",
        "##########",
        "########.#",
        "#########.",
        ".######...",
        "##########",
    ]),
];

/// Noms des dessins, dans le même ordre que `PICTURES`.
#[rustfmt::skip]
const NAMES_EN: [&str; N] = [
    "heart",
    "fir tree",
    "boat",
    "smile",
    "mushroom",
    "umbrella",
    "house",
    "cat",
    "fish",
    "teacup",
];

fn cells(picture: &[&str; N]) -> Vec<Vec<bool>> {
    picture.iter().map(|row| row.chars().map(|c| c == '#' || c == '~').collect()).collect()
}

/// Indices d'une ligne : longueurs des suites de cases noires (`[0]` pour une ligne vide).
fn clues(line: impl Iterator<Item = bool>) -> Vec<u32> {
    let mut out = Vec::new();
    let mut run = 0;
    for filled in line {
        if filled {
            run += 1;
        } else if run > 0 {
            out.push(run);
            run = 0;
        }
    }
    if run > 0 {
        out.push(run);
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

/// Échelle des chiffres des indices (5×7 points à l'échelle 1).
const SCALE: i64 = 3;
/// Hauteur d'un indice de colonne, espacement compris.
const CLUE_H: i64 = draw::DIGIT_H * SCALE + 6;
/// Espace entre deux indices d'une même ligne.
const CLUE_GAP: i64 = 4 * SCALE;
/// Espace entre les indices et la grille.
const CLUE_PAD: i64 = 10;
/// Marge de chaque côté du papier.
const MARGIN: i64 = 8;
/// Taille maximale d'une case : au-delà, la grille devient inutilement grande.
const MAX_CELL: i64 = 44;

/// Largeur, en points, des indices d'une ligne.
fn row_clue_width(clue: &[u32]) -> i64 {
    clue.iter().map(|&n| draw::number_width(n, SCALE)).sum::<i64>() + (clue.len() as i64 - 1) * CLUE_GAP
}

fn render(grid: &[Vec<bool>], solution: bool) -> GrayImage {
    let rows: Vec<Vec<u32>> = grid.iter().map(|r| clues(r.iter().copied())).collect();
    let cols: Vec<Vec<u32>> = (0..N).map(|x| clues(grid.iter().map(|r| r[x]))).collect();

    // La case la plus grande qui laisse la place aux indices, puis l'ensemble (indices + grille) centré.
    let width = i64::from(PRINT_WIDTH);
    let clue_w = rows.iter().map(|r| row_clue_width(r)).max().unwrap_or(0);
    let cell = ((width - 2 * MARGIN - clue_w - CLUE_PAD) / N as i64).min(MAX_CELL);
    let size = cell * N as i64;
    let left = (width - (clue_w + CLUE_PAD + size)) / 2;
    let top = cols.iter().map(Vec::len).max().unwrap_or(1) as i64 * CLUE_H + 6;
    let (ox, oy) = (left + clue_w + CLUE_PAD, top);
    let mut img = draw::canvas((oy + size + 8) as u32);

    // Indices des colonnes, empilés au-dessus de la grille, alignés en bas.
    for (x, clue) in cols.iter().enumerate() {
        let cx = ox + x as i64 * cell + cell / 2;
        for (k, &n) in clue.iter().rev().enumerate() {
            let y = oy - (k as i64 + 1) * CLUE_H;
            draw::number(&mut img, n, cx - draw::number_width(n, SCALE) / 2, y, SCALE);
        }
    }
    // Indices des lignes, à gauche, alignés à droite contre la grille.
    for (y, clue) in rows.iter().enumerate() {
        let cy = oy + y as i64 * cell + (cell - draw::DIGIT_H * SCALE) / 2;
        let mut x = ox - CLUE_PAD;
        for &n in clue.iter().rev() {
            x -= draw::number_width(n, SCALE);
            draw::number(&mut img, n, x, cy, SCALE);
            x -= CLUE_GAP;
        }
    }
    if solution {
        for (y, row) in grid.iter().enumerate() {
            for (x, &filled) in row.iter().enumerate() {
                if filled {
                    draw::fill_rect(&mut img, ox + x as i64 * cell, oy + y as i64 * cell, cell, cell);
                }
            }
        }
    }
    // Quadrillage : traits épais tous les cinq.
    for k in 0..=N as i64 {
        let thick = if k % 5 == 0 { 4 } else { 1 };
        draw::fill_rect(&mut img, ox + k * cell - thick / 2, oy, thick, size);
        draw::fill_rect(&mut img, ox, oy + k * cell - thick / 2, size, thick);
    }
    img
}

pub fn build(number: Option<usize>, solution: bool) -> anyhow::Result<Doc> {
    let index = match number {
        Some(n) if (1..=PICTURES.len()).contains(&n) => n - 1,
        Some(n) => anyhow::bail!(
            "{}",
            tr!("logimage n° {n} inconnu (1 à {})", "nonogram No. {n} unknown (1 to {})", PICTURES.len())
        ),
        None => rand::rng().random_range(0..PICTURES.len()),
    };
    let (name_fr, picture) = &PICTURES[index];
    let name = crate::i18n::t(name_fr, NAMES_EN[index]);
    let grid = cells(picture);

    let mut doc = Doc::new();
    doc.header(if solution {
        crate::i18n::t("Solution du logimage", "Nonogram solution")
    } else {
        crate::i18n::t("Logimage", "Nonogram")
    });
    if solution {
        doc.text(&tr!("C'était… {name} !", "It was: {name}!"), Style::default().small().center());
    } else {
        doc.text(
            crate::i18n::t(
                "Noircis les cases : les nombres donnent les suites de cases noires, dans l'ordre.",
                "Shade the cells: the numbers give the runs of shaded cells, in order.",
            ),
            Style::default().small().center(),
        );
    }
    doc.feed(1);
    doc.image(render(&grid, solution));
    doc.text(&tr!("n° {}", "No. {}", index + 1), Style::default().small().align(Align::Right));
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toutes les façons de placer les suites `clue` dans une ligne compatible avec `known`.
    fn placements(clue: &[u32], known: &[Option<bool>]) -> Vec<Vec<bool>> {
        fn go(clue: &[u32], known: &[Option<bool>], start: usize, line: &mut Vec<bool>, out: &mut Vec<Vec<bool>>) {
            let n = known.len();
            let Some((&first, rest)) = clue.split_first() else {
                if (start..n).all(|i| known[i] != Some(true)) {
                    let mut full = line.clone();
                    full.resize(n, false);
                    out.push(full);
                }
                return;
            };
            let len = first as usize;
            let needed: usize = rest.iter().map(|&r| r as usize + 1).sum();
            for pos in start..=n.saturating_sub(len + needed) {
                if (start..pos).any(|i| known[i] == Some(true)) {
                    break;
                }
                if (pos..pos + len).any(|i| known[i] == Some(false)) {
                    continue;
                }
                let end = pos + len;
                if end < n && known[end] == Some(true) {
                    continue;
                }
                let mark = line.len();
                line.resize(pos, false);
                line.extend(std::iter::repeat_n(true, len));
                if end < n {
                    line.push(false);
                }
                go(rest, known, (end + 1).min(n), line, out);
                line.truncate(mark);
            }
        }
        let clue: &[u32] = if clue == [0] { &[] } else { clue };
        let mut out = Vec::new();
        go(clue, known, 0, &mut Vec::new(), &mut out);
        out
    }

    /// Résout par déduction ligne à ligne ; `None` si la grille reste ambiguë.
    fn solve(rows: &[Vec<u32>], cols: &[Vec<u32>]) -> Option<Vec<Vec<bool>>> {
        let mut grid = vec![vec![None; N]; N];
        loop {
            let mut changed = false;
            for vertical in [false, true] {
                for i in 0..N {
                    let line: Vec<Option<bool>> =
                        (0..N).map(|j| if vertical { grid[j][i] } else { grid[i][j] }).collect();
                    let options = placements(if vertical { &cols[i] } else { &rows[i] }, &line);
                    assert!(!options.is_empty(), "grille contradictoire");
                    for j in 0..N {
                        if line[j].is_none() && options.iter().all(|o| o[j] == options[0][j]) {
                            let cell = if vertical { &mut grid[j][i] } else { &mut grid[i][j] };
                            *cell = Some(options[0][j]);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        grid.into_iter().map(|row| row.into_iter().collect()).collect()
    }

    #[test]
    fn every_picture_has_a_unique_logical_solution() {
        let mut ambiguous = Vec::new();
        for (name, picture) in PICTURES {
            assert!(picture.iter().all(|r| r.chars().count() == N), "{name} : ligne de mauvaise taille");
            let grid = cells(picture);
            let rows: Vec<Vec<u32>> = grid.iter().map(|r| clues(r.iter().copied())).collect();
            let cols: Vec<Vec<u32>> = (0..N).map(|x| clues(grid.iter().map(|r| r[x]))).collect();
            if solve(&rows, &cols).as_ref() != Some(&grid) {
                ambiguous.push(*name);
            }
        }
        assert!(ambiguous.is_empty(), "solution ambiguë : {ambiguous:?}");
    }

    #[test]
    fn grid_is_centered_and_fills_the_width() {
        for (name, picture) in PICTURES {
            let img = render(&cells(picture), false);
            let ink: Vec<u32> =
                (0..img.width()).filter(|&x| (0..img.height()).any(|y| img.get_pixel(x, y).0[0] == 0)).collect();
            let (first, last) = (ink[0], *ink.last().unwrap());
            let (gap_left, gap_right) = (first, img.width() - 1 - last);
            assert!(gap_left.abs_diff(gap_right) <= 8, "{name} : décentré ({gap_left} / {gap_right})");
            assert!(last - first >= 440, "{name} : trop étroit ({} points)", last - first);
        }
    }

    #[test]
    fn clues_count_runs() {
        assert_eq!(clues("##.#..###.".chars().map(|c| c == '#')), vec![2, 1, 3]);
        assert_eq!(clues("..........".chars().map(|c| c == '#')), vec![0]);
    }
}
