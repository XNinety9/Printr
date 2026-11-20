//! Bataille navale : pour chaque joueur, sa flotte déjà placée au hasard et une grille vide pour
//! noter ses tirs. Grille de 5 à 10 cases (A-J, 1-10 pour la classique), flotte adaptée à la
//! taille, navires qui ne se touchent pas, même en diagonale.

use image::GrayImage;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::doc::{Doc, Style};
use crate::draw::{self, BLACK};
use crate::i18n;
use crate::raster::PRINT_WIDTH;

pub const SIZES: std::ops::RangeInclusive<u8> = 5..=10;

/// Longueur des navires, du plus grand au plus petit, selon la taille de la grille.
fn fleet(size: u8) -> &'static [u8] {
    match size {
        10 => &[5, 4, 3, 3, 2],
        9 => &[4, 3, 3, 2, 2],
        8 => &[4, 3, 2, 2],
        7 => &[3, 3, 2],
        6 => &[3, 2, 2],
        _ => &[3, 2],
    }
}

/// Nom d'un navire : `nth` distingue deux navires de même longueur.
fn ship_name(len: u8, nth: usize) -> &'static str {
    match (len, nth) {
        (5, _) => i18n::t("Porte-avions", "Carrier"),
        (4, _) => i18n::t("Croiseur", "Battleship"),
        (3, 0) => i18n::t("Contre-torpilleur", "Cruiser"),
        (3, _) => i18n::t("Sous-marin", "Submarine"),
        (2, 0) => i18n::t("Torpilleur", "Destroyer"),
        _ => i18n::t("Patrouilleur", "Patrol boat"),
    }
}

/// Un navire : sa première case (colonne, ligne), sa longueur et son sens.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ship {
    col: u8,
    row: u8,
    len: u8,
    vertical: bool,
}

impl Ship {
    fn cells(self) -> impl Iterator<Item = (u8, u8)> {
        (0..self.len).map(move |i| if self.vertical { (self.col, self.row + i) } else { (self.col + i, self.row) })
    }

    fn last(self) -> (u8, u8) {
        self.cells().last().expect("navire non vide")
    }
}

/// Place la flotte au hasard, sans que deux navires se touchent (même par un coin).
fn place(size: u8, rng: &mut StdRng) -> Vec<Ship> {
    'attempt: loop {
        let mut taken = vec![false; usize::from(size) * usize::from(size)];
        let mut ships = Vec::new();
        for &len in fleet(size) {
            let mut placed = false;
            for _ in 0..200 {
                let vertical = rng.random_bool(0.5);
                let (w, h) = if vertical { (size, size - len + 1) } else { (size - len + 1, size) };
                let ship = Ship { col: rng.random_range(0..w), row: rng.random_range(0..h), len, vertical };
                if ship.cells().all(|(c, r)| !taken[usize::from(r) * usize::from(size) + usize::from(c)]) {
                    // La case du navire et ses huit voisines sont désormais interdites.
                    for (c, r) in ship.cells() {
                        for dr in -1i16..=1 {
                            for dc in -1i16..=1 {
                                let (nc, nr) = (i16::from(c) + dc, i16::from(r) + dr);
                                if (0..i16::from(size)).contains(&nc) && (0..i16::from(size)).contains(&nr) {
                                    taken[nr as usize * usize::from(size) + nc as usize] = true;
                                }
                            }
                        }
                    }
                    ships.push(ship);
                    placed = true;
                    break;
                }
            }
            if !placed {
                continue 'attempt;
            }
        }
        return ships;
    }
}

/// Mise en page d'une grille, en points.
struct Layout {
    cell: i64,
    ox: i64,
    oy: i64,
    scale: i64,
}

fn layout(size: u8) -> Layout {
    let scale = 3;
    let margin = 3 * draw::DIGIT_W * scale;
    let cell = ((i64::from(PRINT_WIDTH) - margin - 12) / i64::from(size)).min(64);
    let width = cell * i64::from(size);
    // La grille est centrée, les numéros de ligne à sa gauche.
    let ox = (i64::from(PRINT_WIDTH) - width + margin) / 2;
    Layout { cell, ox, oy: draw::DIGIT_H * scale + 12, scale }
}

/// Une grille vide, avec les lettres en haut et les numéros à gauche.
fn grid(size: u8) -> (GrayImage, Layout) {
    let l = layout(size);
    let n = i64::from(size);
    let mut img = draw::canvas((l.oy + n * l.cell + 4) as u32);
    for i in 0..=n {
        let w = if i == 0 || i == n { 4 } else { 2 };
        draw::fill_rect(&mut img, l.ox + i * l.cell - w / 2, l.oy - w / 2, w, n * l.cell + w);
        draw::fill_rect(&mut img, l.ox - w / 2, l.oy + i * l.cell - w / 2, n * l.cell + w, w);
    }
    let h = draw::DIGIT_H * l.scale;
    for i in 0..n {
        let letter = b'A' + i as u8;
        let x = l.ox + i * l.cell + (l.cell - draw::DIGIT_W * l.scale) / 2;
        draw::letter(&mut img, letter, x, l.oy - h - 8, l.scale, BLACK);
        let number = (i + 1) as u32;
        let x = l.ox - 8 - draw::number_width(number, l.scale);
        draw::number(&mut img, number, x, l.oy + i * l.cell + (l.cell - h) / 2, l.scale);
    }
    (img, l)
}

/// Les navires, en coques creuses : on peut y marquer les coups reçus.
fn fleet_grid(size: u8, ships: &[Ship]) -> GrayImage {
    let (mut img, l) = grid(size);
    let center = |(c, r): (u8, u8)| {
        (l.ox as f64 + (f64::from(c) + 0.5) * l.cell as f64, l.oy as f64 + (f64::from(r) + 0.5) * l.cell as f64)
    };
    let radius = l.cell as f64 * 0.36;
    let stroke = 4.0;
    for ship in ships {
        let ((x1, y1), (x2, y2)) = (center((ship.col, ship.row)), center(ship.last()));
        let (dx, dy) = (x2 - x1, y2 - y1);
        let len2 = (dx * dx + dy * dy).max(1e-9);
        let (px0, py0) = ((x1.min(x2) - radius) as i64, (y1.min(y2) - radius) as i64);
        let (px1, py1) = ((x1.max(x2) + radius) as i64 + 1, (y1.max(y2) + radius) as i64 + 1);
        for py in py0..=py1 {
            for px in px0..=px1 {
                let (x, y) = (px as f64 + 0.5, py as f64 + 0.5);
                let t = (((x - x1) * dx + (y - y1) * dy) / len2).clamp(0.0, 1.0);
                let d = (x - (x1 + t * dx)).hypot(y - (y1 + t * dy));
                if d <= radius && d > radius - stroke {
                    img.put_pixel(px as u32, py as u32, BLACK);
                }
            }
        }
    }
    img
}

/// La liste des navires adverses, à cocher quand ils sont coulés.
fn checklist(size: u8) -> Vec<String> {
    let mut seen: Vec<u8> = Vec::new();
    fleet(size)
        .iter()
        .map(|&len| {
            let nth = seen.iter().filter(|&&l| l == len).count();
            seen.push(len);
            format!("[ ] {} ({len})", ship_name(len, nth))
        })
        .collect()
}

pub struct Options {
    /// Côté de la grille, de 5 à 10 cases.
    pub size: u8,
    /// 1 ou 2 joueurs.
    pub players: u8,
    /// La liste des navires adverses, à cocher.
    pub checklist: bool,
    /// La règle du jeu, en quelques lignes.
    pub rules: bool,
    pub seed: Option<u64>,
}

pub fn build(o: &Options) -> anyhow::Result<Doc> {
    let (size, players) = (o.size, o.players);
    anyhow::ensure!(
        SIZES.contains(&size),
        "{}",
        tr!("grille de {} à {} cases de côté", "grid of {} to {} cells per side", SIZES.start(), SIZES.end())
    );
    anyhow::ensure!((1..=2).contains(&players), "{}", i18n::t("1 ou 2 joueurs", "1 or 2 players"));
    let seed = o.seed.unwrap_or_else(|| rand::rng().random());
    let mut rng = StdRng::seed_from_u64(seed);
    let last = char::from(b'A' + size - 1);
    let mut doc = Doc::new();
    for player in 1..=players {
        if player > 1 {
            doc.cut();
        }
        doc.header(i18n::t("Bataille navale", "Battleship"));
        if players > 1 {
            doc.text(&tr!("Joueur {player}", "Player {player}"), Style::default().bold().size(2).center());
        }
        doc.text(i18n::t("Ma flotte", "My fleet"), Style::default().bold());
        doc.image(fleet_grid(size, &place(size, &mut rng)));
        doc.feed(1);
        doc.text(i18n::t("Mes tirs", "My shots"), Style::default().bold());
        doc.image(grid(size).0);
        doc.text(i18n::t("O : à l'eau - X : touché", "O: miss - X: hit"), Style::default().small().center());
        if o.checklist {
            doc.feed(1);
            doc.text(i18n::t("Navires adverses coulés", "Enemy ships sunk"), Style::default().bold());
            for line in checklist(size) {
                doc.text(&line, Style::default());
            }
        }
        if !o.rules {
            continue;
        }
        doc.feed(1);
        doc.text(
            &tr!(
                "À tour de rôle, annonce une case (de A1 à {last}{size}). L'adversaire répond « à l'eau », « touché » ou « coulé » : note un O ou un X sur tes tirs. Quand il tire sur ta flotte, marque ses coups sur ta grille. Le premier qui coule toute la flotte adverse a gagné !",
                "Take turns calling a cell (A1 to {last}{size}). Your opponent answers “miss”, “hit” or “sunk”: mark an O or an X on your shots. When they fire at your fleet, mark their shots on your grid. The first to sink the whole enemy fleet wins!"
            ),
            Style::default().small(),
        );
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Op;

    fn options(size: u8, players: u8, seed: Option<u64>) -> Options {
        Options { size, players, checklist: true, rules: true, seed }
    }

    #[test]
    fn ships_fit_and_never_touch() {
        for size in SIZES {
            for seed in 0..200 {
                let ships = place(size, &mut StdRng::seed_from_u64(seed));
                assert_eq!(ships.iter().map(|s| s.len).collect::<Vec<_>>(), fleet(size));
                let cells: Vec<Vec<(u8, u8)>> = ships.iter().map(|s| s.cells().collect()).collect();
                for (i, a) in cells.iter().enumerate() {
                    assert!(a.iter().all(|&(c, r)| c < size && r < size), "hors grille ({size}, {seed})");
                    for b in &cells[i + 1..] {
                        let touch = a
                            .iter()
                            .any(|&(c1, r1)| b.iter().any(|&(c2, r2)| c1.abs_diff(c2) <= 1 && r1.abs_diff(r2) <= 1));
                        assert!(!touch, "navires qui se touchent ({size}, {seed})");
                    }
                }
            }
        }
    }

    #[test]
    fn two_players_get_two_tickets_with_different_fleets() {
        let doc = build(&options(10, 2, Some(7))).unwrap();
        assert_eq!(doc.ops.iter().filter(|op| matches!(op, Op::Cut)).count(), 1);
        let fleets: Vec<&GrayImage> =
            doc.ops.iter().filter_map(|op| if let Op::Image(i) = op { Some(i) } else { None }).collect();
        assert_eq!(fleets.len(), 4);
        assert_ne!(fleets[0], fleets[2], "même flotte pour les deux joueurs");
        assert_eq!(fleets[1], fleets[3], "grilles de tirs vides et identiques");
    }

    #[test]
    fn grids_fit_the_paper_and_limits_hold() {
        for size in SIZES {
            let l = layout(size);
            assert!(
                l.ox - 3 * draw::DIGIT_W * l.scale >= 0
                    && l.ox + l.cell * i64::from(size) + 2 <= i64::from(PRINT_WIDTH)
            );
            assert!(build(&options(size, 1, None)).is_ok());
        }
        assert!(build(&options(4, 2, None)).is_err() && build(&options(11, 2, None)).is_err());
        assert!(build(&options(10, 3, None)).is_err());
        assert_eq!(checklist(10)[3], format!("[ ] {} (3)", ship_name(3, 1)));
    }

    #[test]
    fn checklist_and_rules_are_optional() {
        let lines = |o: &Options| build(o).unwrap().ops.iter().filter(|op| matches!(op, Op::Line { .. })).count();
        let full = lines(&options(10, 1, Some(1)));
        let bare = lines(&Options { checklist: false, rules: false, ..options(10, 1, Some(1)) });
        let no_rules = lines(&Options { rules: false, ..options(10, 1, Some(1)) });
        assert!(bare < no_rules && no_rules < full);
    }
}
