//! Patron de dé à découper et plier : une croix de six faces, avec ses languettes de collage.

use anyhow::Result;

use super::{Dash, Scale, Sheet, check_range};
use crate::doc::{Doc, Style};
use crate::draw::BLACK;
use crate::i18n;

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

/// Faces (colonne, ligne, valeur) : les faces opposées font 7.
const FACES: [(i32, i32, u8); 6] = [(1, 0, 2), (0, 1, 3), (1, 1, 1), (2, 1, 4), (1, 2, 5), (1, 3, 6)];

/// Bords qui portent une languette : une par arête à coller.
const TABS: [(i32, i32, Side); 7] = [
    (0, 1, Side::Top),
    (0, 1, Side::Bottom),
    (0, 1, Side::Left),
    (2, 1, Side::Top),
    (2, 1, Side::Bottom),
    (2, 1, Side::Right),
    (1, 3, Side::Bottom),
];

/// Points d'une face, en fraction du côté.
fn pips(value: u8) -> &'static [(f64, f64)] {
    const L: f64 = 0.27;
    const M: f64 = 0.5;
    const H: f64 = 0.73;
    match value {
        1 => &[(M, M)],
        2 => &[(L, L), (H, H)],
        3 => &[(L, L), (M, M), (H, H)],
        4 => &[(L, L), (H, L), (L, H), (H, H)],
        5 => &[(L, L), (H, L), (M, M), (L, H), (H, H)],
        _ => &[(L, L), (H, L), (L, M), (H, M), (L, H), (H, H)],
    }
}

/// `face` : côté d'une face, en mm. Sans `pips`, un dé vierge à décorer.
pub fn build(face: f64, pips_on: bool, tabs: bool, s: Scale) -> Result<Doc> {
    check_range(i18n::t("côté d'une face", "face size"), face, 10.0, 18.0, "mm")?;
    let f = face;
    let tab = if tabs { (f * 0.3).min(4.5) } else { 0.0 };
    let ox = (s.width_mm() - 3.0 * f) / 2.0;
    let oy = 1.0;
    let mut sheet = Sheet::new(s, 4.0 * f + tab + 2.0);
    let has_face = |c: i32, r: i32| FACES.iter().any(|&(fc, fr, _)| (fc, fr) == (c, r));
    let line = 0.3;
    for &(c, r, value) in &FACES {
        let (x, y) = (ox + f64::from(c) * f, oy + f64::from(r) * f);
        let edges = [
            (Side::Top, (x, y), (x + f, y), (0.0, -1.0), (c, r - 1)),
            (Side::Right, (x + f, y), (x + f, y + f), (1.0, 0.0), (c + 1, r)),
            (Side::Bottom, (x + f, y + f), (x, y + f), (0.0, 1.0), (c, r + 1)),
            (Side::Left, (x, y + f), (x, y), (-1.0, 0.0), (c - 1, r)),
        ];
        for (side, p, q, (nx, ny), (nc, nr)) in edges {
            if has_face(nc, nr) {
                // Pli entre deux faces, tracé une seule fois.
                if matches!(side, Side::Right | Side::Bottom) {
                    sheet.line(p, q, line, Some(Dash::FOLD));
                }
            } else if tabs && TABS.contains(&(c, r, side)) {
                sheet.line(p, q, line, Some(Dash::FOLD));
                let (ux, uy) = ((q.0 - p.0) / f, (q.1 - p.1) / f);
                let a = (p.0 + nx * tab + ux * tab, p.1 + ny * tab + uy * tab);
                let b = (q.0 + nx * tab - ux * tab, q.1 + ny * tab - uy * tab);
                sheet.line(p, a, line, None);
                sheet.line(a, b, line, None);
                sheet.line(b, q, line, None);
            } else {
                sheet.line(p, q, line, None);
            }
        }
        if pips_on {
            for &(px, py) in pips(value) {
                sheet.disc((x + px * f, y + py * f), f * 0.09, BLACK);
            }
        }
    }
    let mut doc = Doc::new();
    doc.image(sheet.into_image());
    doc.text(
        i18n::t(
            "Découpe le contour, plie les pointillés, colle les languettes.",
            "Cut along the outline, fold the dotted lines, glue the tabs.",
        ),
        Style::default().small().center(),
    );
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::super::tests::tm88;
    use super::*;

    #[test]
    fn opposite_faces_add_up_to_seven() {
        // Dans la croix, la colonne du milieu fait le tour : faces 0 et 2, 1 et 3 opposées ;
        // les deux faces de côté aussi.
        let value = |c, r| FACES.iter().find(|f| (f.0, f.1) == (c, r)).unwrap().2;
        assert_eq!(value(1, 0) + value(1, 2), 7);
        assert_eq!(value(1, 1) + value(1, 3), 7);
        assert_eq!(value(0, 1) + value(2, 1), 7);
    }

    #[test]
    fn die_fits_the_paper() {
        for paper in [512, 576] {
            assert!(build(18.0, true, true, Scale::for_paper(paper, 1000)).is_ok());
        }
        assert!(build(19.0, true, true, tm88()).is_err());
    }
}
