//! Papier à musique : des portées (4 à 8 lignes : portée classique ou tablature de la basse à la
//! guitare 8 cordes), avec leur clé dessinée à la volée. En largeur, les portées traversent le
//! papier les unes sous les autres ; en longueur, elles courent dans le sens du papier, côte à
//! côte, et se lisent en tournant le ticket.

use anyhow::Result;
use image::GrayImage;
use serde::Deserialize;

use super::{Scale, Sheet, check_range};
use crate::doc::Doc;
use crate::draw::{self, BLACK, WHITE};
use crate::i18n;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Clef {
    #[default]
    #[serde(alias = "sol")]
    Treble,
    #[serde(alias = "fa")]
    Bass,
    #[serde(alias = "ut", alias = "do")]
    Alto,
    #[serde(alias = "tablature")]
    Tab,
    #[serde(alias = "aucune", alias = "sans")]
    None,
}

impl Clef {
    pub fn label(self) -> &'static str {
        match self {
            Clef::Treble => i18n::t("clé de sol", "treble clef"),
            Clef::Bass => i18n::t("clé de fa", "bass clef"),
            Clef::Alto => i18n::t("clé d'ut", "alto clef"),
            Clef::Tab => i18n::t("tablature", "tablature"),
            Clef::None => i18n::t("sans clé", "no clef"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// Portées d'un bord à l'autre du papier, les unes sous les autres.
    #[default]
    #[serde(alias = "largeur", alias = "en_largeur", alias = "lignes")]
    Across,
    /// Portées dans le sens du papier, aussi longues qu'on veut.
    #[serde(alias = "longueur", alias = "en_longueur")]
    Along,
}

pub struct Options {
    /// Lignes de chaque portée, de 4 à 8.
    pub lines: u8,
    pub clef: Clef,
    pub layout: Layout,
    /// En largeur : nombre de portées (1 à 30).
    pub staves: u8,
    /// En longueur : longueur du papier, en cm (5 à 100), pour deux portées côte à côte.
    pub length: u16,
}

/// Une portée vue « à l'endroit » : `u` le long des lignes, `v` en travers (vers le bas), en mm,
/// et sa place sur le papier.
struct View {
    vertical: bool,
    /// Point du papier où tombe (u, v) = (0, 0).
    origin: (f64, f64),
}

impl View {
    /// En longueur, on lit le ticket tourné d'un quart de tour vers la gauche : le haut du papier
    /// devient la gauche de la portée, son bord gauche le bas.
    fn at(&self, u: f64, v: f64) -> (f64, f64) {
        if self.vertical { (self.origin.0 - v, self.origin.1 + u) } else { (self.origin.0 + u, self.origin.1 + v) }
    }
}

/// Lisse une suite de points (spline de Catmull-Rom), pour tracer une courbe régulière.
fn smooth(points: &[(f64, f64)], steps: usize) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for i in 0..points.len() - 1 {
        let p0 = points[i.saturating_sub(1)];
        let (p1, p2) = (points[i], points[i + 1]);
        let p3 = points[(i + 2).min(points.len() - 1)];
        for k in 0..steps {
            let t = k as f64 / steps as f64;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f64, b: f64, c: f64, d: f64| {
                0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (3.0 * b - a - 3.0 * c + d) * t3)
            };
            out.push((f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1)));
        }
    }
    out.push(*points.last().expect("au moins un point"));
    out
}

/// Clé de sol, en interlignes, autour de la ligne de sol : la hampe descend du haut de la
/// boucle, puis la courbe repasse à droite et s'enroule autour de la ligne de sol.
#[rustfmt::skip]
const TREBLE_STEM: [(f64, f64); 6] = [
    (0.35, -4.7), (0.15, -2.5), (0.0, 0.0), (0.0, 2.3), (-0.3, 2.9), (-0.85, 2.6),
];
#[rustfmt::skip]
const TREBLE_BODY: [(f64, f64); 15] = [
    (0.35, -4.7), (0.85, -4.2), (0.8, -3.3), (0.2, -2.4), (-0.7, -1.6), (-1.25, -0.6), (-1.15, 0.45),
    (-0.45, 1.05), (0.5, 1.05), (1.1, 0.45), (1.0, -0.35), (0.4, -0.7), (-0.25, -0.45), (-0.25, 0.1),
    (0.15, 0.25),
];
/// Clé de fa, autour de la ligne de fa : la tête, puis l'arc qui descend vers la gauche.
#[rustfmt::skip]
const BASS: [(f64, f64); 8] = [
    (-0.9, 0.0), (-0.6, -0.8), (0.2, -1.1), (1.0, -0.6), (1.1, 0.4), (0.6, 1.4), (-0.3, 2.2), (-1.0, 2.6),
];
/// Clé d'ut : la moitié du haut d'une des deux courbes, autour de la ligne d'ut (l'autre est
/// son reflet).
#[rustfmt::skip]
const ALTO_CURVE: [(f64, f64); 6] = [
    (0.0, 0.0), (0.45, -0.55), (1.05, -0.9), (1.35, -1.5), (1.15, -1.95), (0.7, -1.85),
];

fn path(sheet: &mut Sheet, view: &View, anchor: (f64, f64), sp: f64, points: &[(f64, f64)], width: f64) {
    let curve = smooth(points, 8);
    for pair in curve.windows(2) {
        let a = view.at(anchor.0 + pair[0].0 * sp, anchor.1 + pair[0].1 * sp);
        let b = view.at(anchor.0 + pair[1].0 * sp, anchor.1 + pair[1].1 * sp);
        sheet.line(a, b, width, None);
    }
}

fn dot(sheet: &mut Sheet, view: &View, anchor: (f64, f64), sp: f64, (x, y): (f64, f64), r: f64) {
    sheet.disc(view.at(anchor.0 + x * sp, anchor.1 + y * sp), r * sp, BLACK);
}

/// Colle le texte `s`, centré sur `center` (papier), tourné comme la portée.
fn letters(sheet: &mut Sheet, view: &View, s: &str, center: (f64, f64), scale: i64) {
    let (w, h) = (draw::text_width(s, scale), draw::DIGIT_H * scale);
    let mut img = GrayImage::from_pixel(w as u32, h as u32, WHITE);
    draw::text(&mut img, s, 0, 0, scale);
    if view.vertical {
        img = image::imageops::rotate90(&img);
    }
    let (px, py) = (sheet.s.dx(center.0) - i64::from(img.width()) / 2, sheet.s.dy(center.1) - i64::from(img.height()) / 2);
    for (x, y, p) in img.enumerate_pixels() {
        let (tx, ty) = (px + i64::from(x), py + i64::from(y));
        if p.0[0] == 0 && tx >= 0 && ty >= 0 && tx < i64::from(sheet.img.width()) && ty < i64::from(sheet.img.height()) {
            sheet.img.put_pixel(tx as u32, ty as u32, BLACK);
        }
    }
}

/// Une portée de `len` mm, dont la ligne du haut est en v = 0.
fn staff(sheet: &mut Sheet, view: &View, len: f64, lines: u8, sp: f64, clef: Clef) {
    let height = f64::from(lines - 1) * sp;
    for k in 0..lines {
        let v = f64::from(k) * sp;
        sheet.line(view.at(0.0, v), view.at(len, v), 0.2, None);
    }
    for u in [0.0, len] {
        sheet.line(view.at(u, 0.0), view.at(u, height), 0.3, None);
    }
    let stroke = 0.22 * sp;
    match clef {
        Clef::Treble => {
            // Sur la deuxième ligne en partant du bas.
            let anchor = (2.0 * sp, f64::from(lines - 2) * sp);
            path(sheet, view, anchor, sp, &TREBLE_STEM, stroke * 0.8);
            path(sheet, view, anchor, sp, &TREBLE_BODY, stroke);
            dot(sheet, view, anchor, sp, (-0.75, 2.45), 0.3);
        }
        Clef::Bass => {
            // Sur la deuxième ligne en partant du haut.
            let anchor = (2.0 * sp, sp);
            path(sheet, view, anchor, sp, &BASS, stroke);
            dot(sheet, view, anchor, sp, (-0.75, 0.05), 0.4);
            dot(sheet, view, anchor, sp, (1.75, -0.5), 0.2);
            dot(sheet, view, anchor, sp, (1.75, 0.5), 0.2);
        }
        Clef::Alto => {
            let anchor = (1.2 * sp, height / 2.0);
            let half = height / 2.0 / sp;
            for (x, w) in [(0.0, 0.45), (0.65, 0.15)] {
                let (a, b) = (view.at(anchor.0 + x * sp, 0.0), view.at(anchor.0 + x * sp, height));
                sheet.line(a, b, w * sp, None);
            }
            // Les deux courbes, étirées à la hauteur de la portée.
            for sign in [-1.0, 1.0] {
                let curve: Vec<(f64, f64)> =
                    ALTO_CURVE.iter().map(|&(x, y)| (x + 0.85, -sign * y.abs() * half / 2.0)).collect();
                path(sheet, view, anchor, sp, &curve, stroke);
                dot(sheet, view, anchor, sp, (curve[5].0 - 0.1, curve[5].1), 0.3);
            }
        }
        Clef::Tab => {
            // T, A, B empilés, à la hauteur de la portée.
            let size = height / 3.3;
            let scale = ((size * sheet.s.y) / 7.0).round().max(1.0) as i64;
            for (i, c) in ["T", "A", "B"].into_iter().enumerate() {
                let v = height / 2.0 + (i as f64 - 1.0) * size * 1.1;
                letters(sheet, view, c, view.at(1.6 * sp, v), scale);
            }
        }
        Clef::None => {}
    }
}

/// Portées côte à côte, en longueur.
const ALONG: u32 = 2;

pub fn build(o: &Options, s: Scale) -> Result<Doc> {
    check_range(i18n::t("lignes par portée", "lines per staff"), f64::from(o.lines), 4.0, 8.0, "")?;
    // Interligne : de quoi écrire à la main, un peu plus pour les chiffres d'une tablature.
    let wide = if o.clef == Clef::Tab { 5.4 } else { 4.5 };
    // Entre deux portées, en interlignes : la clé de sol dépasse en haut comme en bas, et il
    // reste de la place pour annoter.
    let gap_sp = if o.clef == Clef::Treble { 5.0 } else { 3.5 };
    let margin = 2.0;
    let sp = match o.layout {
        Layout::Across => wide,
        // Deux portées côte à côte, aussi grandes que la largeur du papier le permet.
        Layout::Along => {
            let room = s.width_mm() - 2.0 * margin;
            (room / (f64::from(ALONG) * (f64::from(o.lines - 1) + gap_sp))).min(wide)
        }
    };
    let height = f64::from(o.lines - 1) * sp;
    let gap = gap_sp * sp;
    let pitch = height + gap;
    let (sheet, views, len) = match o.layout {
        Layout::Across => {
            check_range(i18n::t("nombre de portées", "number of staves"), f64::from(o.staves), 1.0, 30.0, "")?;
            let sheet = Sheet::new(s, f64::from(o.staves) * pitch);
            let views: Vec<View> = (0..o.staves)
                .map(|i| View { vertical: false, origin: (margin, gap / 2.0 + f64::from(i) * pitch) })
                .collect();
            (sheet, views, s.width_mm() - 2.0 * margin)
        }
        Layout::Along => {
            check_range(i18n::t("longueur du papier", "paper length"), f64::from(o.length), 5.0, 100.0, "cm")?;
            let total = f64::from(o.length) * 10.0;
            let sheet = Sheet::new(s, total);
            let left = (s.width_mm() - f64::from(ALONG) * pitch) / 2.0;
            let views: Vec<View> = (0..ALONG)
                .map(|i| View { vertical: true, origin: (s.width_mm() - left - gap / 2.0 - f64::from(i) * pitch, margin) })
                .collect();
            (sheet, views, total - 2.0 * margin)
        }
    };
    let mut sheet = sheet;
    for view in &views {
        staff(&mut sheet, view, len, o.lines, sp, o.clef);
    }
    let mut doc = Doc::new();
    doc.image(sheet.into_image());
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{black, tm88};
    use super::*;
    use crate::doc::Op;

    fn options() -> Options {
        Options { lines: 5, clef: Clef::Treble, layout: Layout::Across, staves: 3, length: 20 }
    }

    fn image(o: &Options, s: Scale) -> GrayImage {
        let doc = build(o, s).unwrap();
        let Op::Image(img) = &doc.ops[0] else { panic!() };
        img.clone()
    }

    #[test]
    fn across_draws_the_requested_staves() {
        let s = tm88();
        for lines in 4..=8 {
            let img = image(&Options { lines, clef: Clef::None, ..options() }, s);
            // Une colonne au milieu croise toutes les lignes de toutes les portées.
            let column = img.width() / 2;
            let mut count = 0;
            let mut previous = false;
            for y in 0..img.height() {
                let ink = black(&img, column, y);
                if ink && !previous {
                    count += 1;
                }
                previous = ink;
            }
            assert_eq!(count, 3 * u32::from(lines), "{lines} lignes");
        }
    }

    #[test]
    fn along_has_the_requested_length_and_fits() {
        for paper in [512, 576] {
            let s = Scale::for_paper(paper, 1000);
            for clef in [Clef::Treble, Clef::Bass, Clef::Alto, Clef::Tab, Clef::None] {
                for lines in [4, 5, 8] {
                    let o = Options { lines, clef, layout: Layout::Along, length: 30, ..options() };
                    let img = image(&o, s);
                    assert!((f64::from(img.height()) / s.y - 300.0).abs() < 0.5);
                    let (first, last) = (0, img.width() - 1);
                    assert!((0..img.height()).all(|y| !black(&img, first, y) && !black(&img, last, y)), "{clef:?} {lines} déborde");
                }
            }
        }
    }

    #[test]
    fn limits() {
        let s = tm88();
        assert!(build(&Options { lines: 3, ..options() }, s).is_err());
        assert!(build(&Options { lines: 9, ..options() }, s).is_err());
        assert!(build(&Options { staves: 31, ..options() }, s).is_err());
        assert!(build(&Options { layout: Layout::Along, length: 101, ..options() }, s).is_err());
        assert!(build(&Options { staves: 30, lines: 8, ..options() }, s).is_ok());
    }
}
