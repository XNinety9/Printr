//! Illusions d'optique, dessinées à la volée : grille de Hermann, lignes de Zöllner, flèches de
//! Müller-Lyer, cercles d'Ebbinghaus, rails de Ponzo, motif d'Ouchi, triangle de Kanizsa et
//! rayons de Hering. Chacune pose sa question ; la réponse s'imprime à l'envers, en dessous.

use std::f64::consts::PI;

use anyhow::Result;
use serde::Deserialize;

use super::{Scale, Sheet};
use crate::doc::{Doc, Style};
use crate::draw::{BLACK, WHITE};
use crate::i18n;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    #[serde(alias = "grille")]
    Hermann,
    #[serde(alias = "paralleles")]
    Zollner,
    #[serde(alias = "fleches")]
    MullerLyer,
    #[serde(alias = "cercles")]
    Ebbinghaus,
    #[serde(alias = "rails")]
    Ponzo,
    #[serde(alias = "flottant")]
    Ouchi,
    #[serde(alias = "triangle")]
    Kanizsa,
    #[serde(alias = "rayons")]
    Hering,
}

impl Kind {
    #[cfg(test)]
    const ALL: [Kind; 8] = [
        Kind::Hermann,
        Kind::Zollner,
        Kind::MullerLyer,
        Kind::Ebbinghaus,
        Kind::Ponzo,
        Kind::Ouchi,
        Kind::Kanizsa,
        Kind::Hering,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Hermann => i18n::t("grille de Hermann", "Hermann grid"),
            Kind::Zollner => i18n::t("lignes de Zöllner", "Zöllner lines"),
            Kind::MullerLyer => i18n::t("flèches de Müller-Lyer", "Müller-Lyer arrows"),
            Kind::Ebbinghaus => i18n::t("cercles d'Ebbinghaus", "Ebbinghaus circles"),
            Kind::Ponzo => i18n::t("rails de Ponzo", "Ponzo rails"),
            Kind::Ouchi => i18n::t("motif d'Ouchi", "Ouchi pattern"),
            Kind::Kanizsa => i18n::t("triangle de Kanizsa", "Kanizsa triangle"),
            Kind::Hering => i18n::t("rayons de Hering", "Hering rays"),
        }
    }

    fn question(self) -> &'static str {
        match self {
            Kind::Hermann => {
                i18n::t("Vois-tu des taches grises aux croisements ?", "Do you see grey spots at the crossings?")
            }
            Kind::Zollner => i18n::t("Les grandes lignes sont-elles parallèles ?", "Are the long lines parallel?"),
            Kind::MullerLyer => i18n::t("Quel trait est le plus long ?", "Which line is longer?"),
            Kind::Ebbinghaus => i18n::t("Quel disque noir est le plus grand ?", "Which black disc is bigger?"),
            Kind::Ponzo => i18n::t("Quelle barre est la plus longue ?", "Which bar is longer?"),
            Kind::Ouchi => i18n::t(
                "Bouge doucement le ticket : le disque du milieu flotte-t-il ?",
                "Move the ticket gently: does the middle disc float?",
            ),
            Kind::Kanizsa => i18n::t("Vois-tu le triangle blanc, devant ?", "Can you see the white triangle in front?"),
            Kind::Hering => {
                i18n::t("Les deux lignes épaisses sont-elles droites ?", "Are the two thick lines straight?")
            }
        }
    }

    fn answer(self) -> &'static str {
        match self {
            Kind::Hermann => i18n::t(
                "Il n'y en a aucune : ton œil les invente. Fixe un croisement, sa tache disparaît.",
                "There are none: your eye makes them up. Stare at one crossing and its spot vanishes.",
            ),
            Kind::Zollner => i18n::t(
                "Oui, toutes. Ce sont les petits traits qui les font pencher.",
                "Yes, all of them. The short strokes make them look tilted.",
            ),
            Kind::MullerLyer => i18n::t(
                "Aucun : ils ont exactement la même longueur. Vérifie avec une règle !",
                "Neither: they are exactly the same length. Check with a ruler!",
            ),
            Kind::Ebbinghaus => i18n::t(
                "Aucun : ils ont la même taille. Les voisins changent tout.",
                "Neither: they are the same size. The neighbours change everything.",
            ),
            Kind::Ponzo => i18n::t(
                "Aucune : elles sont identiques. Les rails font croire que celle du haut est plus loin, donc plus grande.",
                "Neither: they are identical. The rails make the top one seem farther away, so bigger.",
            ),
            Kind::Ouchi => i18n::t(
                "Rien ne bouge : ce sont les rectangles croisés qui trompent l'œil.",
                "Nothing moves: the crossed rectangles fool the eye.",
            ),
            Kind::Kanizsa => i18n::t(
                "Il n'est pas dessiné : ton cerveau complète les formes.",
                "It isn't drawn: your brain fills in the shapes.",
            ),
            Kind::Hering => i18n::t(
                "Oui, droites et parallèles : ce sont les rayons qui les font paraître courbées.",
                "Yes, straight and parallel: the rays make them look bent.",
            ),
        }
    }
}

/// Largeur de chaque dessin, en mm : elle tient sur toutes les imprimantes.
const W: f64 = 60.0;

/// Le point est-il dans le triangle (a, b, c) ?
fn in_triangle((x, y): (f64, f64), [a, b, c]: [(f64, f64); 3]) -> bool {
    let side = |p: (f64, f64), q: (f64, f64)| (q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0);
    let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

/// Dessine l'illusion dans une feuille ; `ox` est le bord gauche du dessin.
fn draw(kind: Kind, s: Scale) -> image::GrayImage {
    let ox = (s.width_mm() - W) / 2.0;
    let top = 1.0;
    match kind {
        Kind::Hermann => {
            let (n, lane) = (6, 2.4);
            let sq = (W - lane * f64::from(n - 1)) / f64::from(n);
            let mut sheet = Sheet::new(s, W + 2.0);
            for i in 0..n {
                for j in 0..n {
                    let (x, y) = (ox + f64::from(i) * (sq + lane), top + f64::from(j) * (sq + lane));
                    sheet.fill_where((x, y, x + sq, y + sq), BLACK, |px, py| {
                        px >= x && px < x + sq && py >= y && py < y + sq
                    });
                }
            }
            sheet.into_image()
        }
        Kind::Zollner => {
            let mut sheet = Sheet::new(s, W + 2.0);
            let inside = |x: f64, y: f64| (0.0..=W).contains(&x) && (0.0..=W).contains(&y);
            let at = |x: f64, y: f64| (ox + x, top + y);
            // Lignes y = x + k, coupées au carré ; leurs petits traits sont tour à tour
            // horizontaux et verticaux.
            for (n, k) in (-4..=4).map(|i| f64::from(i) * 12.0).enumerate() {
                let (x1, x2) = ((-k).max(0.0), (W - k).min(W));
                sheet.line(at(x1, x1 + k), at(x2, x2 + k), 0.5, None);
                let half = 3.0;
                let mut x = x1 + 1.5;
                while x <= x2 - 1.5 {
                    let (cx, cy) = (x, x + k);
                    let (a, b) = if n % 2 == 0 {
                        ((cx - half, cy), (cx + half, cy))
                    } else {
                        ((cx, cy - half), (cx, cy + half))
                    };
                    if inside(a.0, a.1) && inside(b.0, b.1) {
                        sheet.line(at(a.0, a.1), at(b.0, b.1), 0.4, None);
                    }
                    x += 2.6;
                }
            }
            sheet.into_image()
        }
        Kind::MullerLyer => {
            let (len, fin, angle) = (36.0, 7.0, 35f64.to_radians());
            let (fx, fy) = (fin * angle.cos(), fin * angle.sin());
            let mut sheet = Sheet::new(s, 40.0);
            let (a, b) = (s.width_mm() / 2.0 - len / 2.0, s.width_mm() / 2.0 + len / 2.0);
            // En haut, les ailettes repliées vers l'intérieur ; en bas, ouvertes vers l'extérieur.
            for (y, out) in [(10.0, 1.0), (30.0, -1.0)] {
                sheet.line((a, y), (b, y), 0.6, None);
                for (x, dir) in [(a, 1.0), (b, -1.0)] {
                    for sign in [-1.0, 1.0] {
                        sheet.line((x, y), (x + dir * out * fx, y + sign * fy), 0.6, None);
                    }
                }
            }
            sheet.into_image()
        }
        Kind::Ebbinghaus => {
            let mut sheet = Sheet::new(s, 42.0);
            let cy = 21.0;
            let core = 4.0;
            // À gauche, de grands voisins ; à droite, des petits.
            for (cx, count, r, dist) in [(ox + 19.5, 6, 6.5, 13.0), (ox + 50.0, 8, 2.0, 8.0)] {
                sheet.disc((cx, cy), core, BLACK);
                for i in 0..count {
                    let a = 2.0 * PI * f64::from(i) / f64::from(count);
                    sheet.circle((cx + dist * a.cos(), cy + dist * a.sin()), r, 0.45, None);
                }
            }
            sheet.into_image()
        }
        Kind::Ponzo => {
            let h = 60.0;
            let mut sheet = Sheet::new(s, h + 2.0);
            let cx = s.width_mm() / 2.0;
            let (y0, y1) = (top, top + h);
            let half = |y: f64| 3.0 + (W / 2.0 - 4.0 - 3.0) * (y - y0) / h;
            for sign in [-1.0, 1.0] {
                sheet.line((cx + sign * half(y0), y0), (cx + sign * half(y1), y1), 0.6, None);
            }
            // Traverses, de plus en plus serrées vers le haut.
            let mut y = y1 - 2.0;
            let mut step = 8.0;
            while y > y0 + 2.0 {
                sheet.line((cx - half(y), y), (cx + half(y), y), 0.3, None);
                y -= step;
                step = (step * 0.85).max(2.0);
            }
            let bar = 15.0;
            for y in [top + 15.0, top + 47.0] {
                sheet.fill_where((cx - bar / 2.0, y - 1.0, cx + bar / 2.0, y + 1.0), WHITE, |_, _| true);
                sheet.line((cx - bar / 2.0, y), (cx + bar / 2.0, y), 1.6, None);
            }
            sheet.into_image()
        }
        Kind::Ouchi => {
            let mut sheet = Sheet::new(s, W + 2.0);
            let (long, short) = (4.8, 1.2);
            let (cx, cy, r) = (W / 2.0, W / 2.0, 16.0);
            sheet.fill_where((ox, top, ox + W, top + W), BLACK, |x, y| {
                let (x, y) = (x - ox, y - top);
                if !(0.0..W).contains(&x) || !(0.0..W).contains(&y) {
                    return false;
                }
                // Dehors, des rectangles couchés ; dans le disque, debout.
                let (w, h) = if (x - cx).hypot(y - cy) <= r { (short, long) } else { (long, short) };
                ((x / w).floor() + (y / h).floor()).rem_euclid(2.0) == 0.0
            });
            sheet.into_image()
        }
        Kind::Kanizsa => {
            let mut sheet = Sheet::new(s, 57.0);
            let c = (s.width_mm() / 2.0, 33.0);
            let big = 20.0;
            let vertex = |deg: f64| (c.0 + big * deg.to_radians().cos(), c.1 + big * deg.to_radians().sin());
            let front = [vertex(-90.0), vertex(30.0), vertex(150.0)];
            let back = [vertex(90.0), vertex(210.0), vertex(330.0)];
            sheet.polygon(&back, 0.6, None);
            for p in front {
                sheet.disc(p, 7.0, BLACK);
            }
            // Le triangle « devant » efface tout ce qu'il recouvre : il n'existe que par ses bords.
            let (xs, ys) = (front.map(|p| p.0), front.map(|p| p.1));
            let bbox = (
                xs.iter().copied().fold(f64::MAX, f64::min),
                ys.iter().copied().fold(f64::MAX, f64::min),
                xs.iter().copied().fold(f64::MIN, f64::max),
                ys.iter().copied().fold(f64::MIN, f64::max),
            );
            sheet.fill_where(bbox, WHITE, |x, y| in_triangle((x, y), front));
            sheet.into_image()
        }
        Kind::Hering => {
            let h = 56.0;
            let mut sheet = Sheet::new(s, h + 2.0);
            let c = (s.width_mm() / 2.0, top + h / 2.0);
            let rays = 24;
            for i in 0..rays {
                let a = PI * f64::from(i) / f64::from(rays);
                let (dx, dy) = (a.cos(), a.sin());
                // Jusqu'au bord du cadre, des deux côtés.
                let t = (if dx.abs() > 1e-9 { (W / 2.0) / dx.abs() } else { f64::MAX }).min(if dy.abs() > 1e-9 {
                    (h / 2.0) / dy.abs()
                } else {
                    f64::MAX
                });
                sheet.line((c.0 - t * dx, c.1 - t * dy), (c.0 + t * dx, c.1 + t * dy), 0.25, None);
            }
            for sign in [-1.0, 1.0] {
                let x = c.0 + sign * 9.0;
                sheet.line((x, top), (x, top + h), 1.1, None);
            }
            sheet.into_image()
        }
    }
}

pub fn build(kind: Kind, answer: bool, s: Scale) -> Result<Doc> {
    let mut doc = Doc::new();
    doc.header(kind.label());
    doc.image(draw(kind, s));
    doc.text(kind.question(), Style::default().center());
    if answer {
        doc.feed(1);
        doc.text(kind.answer(), Style::default().small().center().upside_down());
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{black, tm88};
    use super::*;
    use crate::doc::Op;

    #[test]
    fn every_illusion_fits_both_papers() {
        for paper in [512, 576] {
            let s = Scale::for_paper(paper, 1000);
            for kind in Kind::ALL {
                let img = draw(kind, s);
                assert_eq!(img.width(), crate::raster::PRINT_WIDTH);
                // Rien n'est dessiné hors des 60 mm du milieu (à l'épaisseur d'un trait près).
                let margin = (((s.width_mm() - W) / 2.0 - 0.6) * s.x) as u32;
                let stray =
                    (0..img.height()).any(|y| black(&img, margin, y) || black(&img, img.width() - 1 - margin, y));
                assert!(!stray, "{kind:?} déborde sur {paper} points");
                assert!(img.pixels().any(|p| p.0[0] == 0), "{kind:?} est vide");
            }
        }
    }

    #[test]
    fn muller_lyer_lines_are_equal() {
        let s = tm88();
        let img = draw(Kind::MullerLyer, s);
        let span = |y: f64| {
            let row = s.dy(y) as u32;
            let inked: Vec<u32> = (0..img.width()).filter(|&x| black(&img, x, row)).collect();
            inked.last().unwrap() - inked.first().unwrap()
        };
        // En haut, les ailettes rentrent : la ligne seule fait la largeur de la rangée.
        let top = f64::from(span(10.0)) / s.x;
        assert!((top - 36.0).abs() < 0.6, "{top} mm");
    }

    #[test]
    fn answer_is_upside_down() {
        let doc = build(Kind::Kanizsa, true, tm88()).unwrap();
        assert!(doc.ops.iter().any(|op| matches!(op, Op::Line { style, .. } if style.upside_down)));
        let doc = build(Kind::Kanizsa, false, tm88()).unwrap();
        assert!(!doc.ops.iter().any(|op| matches!(op, Op::Line { style, .. } if style.upside_down)));
    }
}
