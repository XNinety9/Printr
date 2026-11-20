//! Règle graduée, en centimètres, en pouces ou les deux, dans le sens du papier (jusqu'à un mètre)
//! ou en travers (60 mm au plus, comme une petite règle).

use anyhow::Result;
use serde::Deserialize;

use super::{MAX_WIDTH_MM, Scale, Sheet};
use crate::doc::{Doc, Style};
use crate::draw::{self, BLACK};
use crate::i18n;
use crate::raster::PRINT_WIDTH;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    #[default]
    #[serde(alias = "centimetres", alias = "centimetre")]
    Cm,
    #[serde(alias = "pouces", alias = "pouce", alias = "in", alias = "inches")]
    Inch,
    /// Centimètres d'un côté, pouces de l'autre ; la longueur se donne en centimètres.
    #[serde(alias = "les_deux", alias = "cm_inch")]
    Both,
}

impl Unit {
    pub fn label(self) -> &'static str {
        match self {
            Unit::Cm => "cm",
            Unit::Inch => i18n::t("pouces", "in"),
            Unit::Both => i18n::t("cm et pouces", "cm and in"),
        }
    }

    /// Longueur d'une unité de `length`, en mm.
    fn mm(self) -> f64 {
        if self == Unit::Inch { 25.4 } else { 10.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Dans le sens du papier : aussi longue qu'on veut.
    #[default]
    #[serde(alias = "verticale")]
    Vertical,
    /// En travers du papier : 60 mm au plus.
    #[serde(alias = "horizontale")]
    Horizontal,
}

/// Longueur la plus grande, dans l'unité de la règle.
pub fn max_length(unit: Unit, orientation: Orientation) -> u16 {
    match (orientation, unit) {
        (Orientation::Horizontal, Unit::Inch) => (MAX_WIDTH_MM / 25.4) as u16,
        (Orientation::Horizontal, _) => (MAX_WIDTH_MM / 10.0) as u16,
        (Orientation::Vertical, Unit::Inch) => 40,
        (Orientation::Vertical, _) => 100,
    }
}

struct Tick {
    at: f64,
    len: f64,
    label: Option<u32>,
    major: bool,
}

/// Graduations en mm (0,5 et 1 cm plus longues), numérotées à chaque centimètre.
fn cm_ticks(length_mm: f64) -> Vec<Tick> {
    (0..=length_mm.round() as u32)
        .map(|k| Tick {
            at: f64::from(k),
            len: if k % 10 == 0 {
                6.0
            } else if k % 5 == 0 {
                4.0
            } else {
                2.5
            },
            label: (k % 10 == 0).then_some(k / 10),
            major: k % 10 == 0,
        })
        .collect()
}

/// Graduations au seizième de pouce, numérotées à chaque pouce.
fn inch_ticks(length_mm: f64) -> Vec<Tick> {
    (0..=(length_mm / 25.4 * 16.0 + 1e-6).floor() as u32)
        .map(|k| Tick {
            at: f64::from(k) * 25.4 / 16.0,
            len: match k {
                k if k % 16 == 0 => 6.0,
                k if k % 8 == 0 => 4.5,
                k if k % 4 == 0 => 3.5,
                k if k % 2 == 0 => 2.5,
                _ => 1.6,
            },
            label: (k % 16 == 0).then_some(k / 16),
            major: k % 16 == 0,
        })
        .collect()
}

pub fn build(length: u16, unit: Unit, orientation: Orientation, s: Scale) -> Result<Doc> {
    let max = max_length(unit, orientation);
    anyhow::ensure!(
        (1..=max).contains(&length),
        "{}",
        match orientation {
            Orientation::Horizontal => tr!(
                "règle horizontale : {max} {} au plus (60 mm, la largeur du papier)",
                "horizontal ruler: {max} {} at most (60 mm, the paper width)",
                unit.label()
            ),
            Orientation::Vertical => tr!("règle : de 1 à {max} {}", "ruler: 1 to {max} {}", unit.label()),
        }
    );
    let length_mm = f64::from(length) * unit.mm();
    let (first, second) = match unit {
        Unit::Cm => (cm_ticks(length_mm), None),
        Unit::Inch => (inch_ticks(length_mm), None),
        Unit::Both => (cm_ticks(length_mm), Some(inch_ticks(length_mm))),
    };
    let mut doc = Doc::new();
    match orientation {
        Orientation::Vertical => {
            doc.image(vertical(s, length_mm, &first, second.as_deref(), unit));
        }
        Orientation::Horizontal => {
            doc.image(horizontal(s, &first, second.as_deref()));
            if unit == Unit::Both {
                doc.text(
                    i18n::t("en haut : cm · en bas : pouces", "top: cm · bottom: inches"),
                    Style::default().small().center(),
                );
            }
        }
    }
    Ok(doc)
}

/// Épaisseur d'un trait de graduation, en points : 2 pour l'unité, 1 pour le reste.
fn dots(tick: &Tick) -> i64 {
    if tick.major { 2 } else { 1 }
}

/// Épaisseur de la ligne de base, en points.
const BASE: i64 = 2;

/// Graduations placées au point près, comme sur une imprimante d'étiquettes : chaque trait
/// commence exactement sur sa graduation et s'étend d'un seul côté (vers la droite, ou vers le
/// bas), et la ligne de base va du début du premier trait à la fin du dernier. Rien ne dépasse.
struct Placed {
    /// Début du trait, en points depuis l'origine de la règle.
    at: i64,
    width: i64,
    len_mm: f64,
    label: Option<u32>,
}

fn place(ticks: &[Tick], dots_per_mm: f64) -> Vec<Placed> {
    ticks
        .iter()
        .map(|t| Placed { at: (t.at * dots_per_mm).round() as i64, width: dots(t), len_mm: t.len, label: t.label })
        .collect()
}

/// Longueur de la ligne de base : jusqu'à la fin du dernier trait.
fn base_len(ticks: &[Placed]) -> i64 {
    ticks.last().map_or(1, |t| t.at + t.width)
}

/// Règle dans le sens du papier : graduations à gauche, pouces à droite pour `Both`.
fn vertical(s: Scale, length_mm: f64, left: &[Tick], right: Option<&[Tick]>, unit: Unit) -> image::GrayImage {
    let top = 4.0;
    let mut sheet = Sheet::new(s, top + length_mm + 9.0);
    let width = s.width_mm();
    let page = i64::from(PRINT_WIDTH);
    let digits = s.digits();
    let y0 = s.dy(top);
    let left = place(left, s.y);
    draw::fill_rect(&mut sheet.img, 0, y0, BASE, base_len(&left));
    for t in &left {
        draw::fill_rect(&mut sheet.img, 0, y0 + t.at, s.dx(t.len_mm), t.width);
        if let Some(n) = t.label {
            let y = (y0 + t.at) as f64 / s.y;
            sheet.label_left(&n.to_string(), (t.len_mm + 1.2, y), digits);
        }
    }
    let (left_unit, right_unit) = match unit {
        Unit::Inch => ("IN", None),
        Unit::Both => ("CM", Some("IN")),
        Unit::Cm => ("CM", None),
    };
    if let Some(right) = right {
        let right = place(right, s.y);
        draw::fill_rect(&mut sheet.img, page - BASE, y0, BASE, base_len(&right));
        for t in &right {
            let len = s.dx(t.len_mm);
            draw::fill_rect(&mut sheet.img, page - len, y0 + t.at, len, t.width);
            if let Some(n) = t.label {
                let y = (y0 + t.at) as f64 / s.y;
                sheet.label_right(&n.to_string(), (width - t.len_mm - 1.2, y), digits);
            }
        }
    }
    let y = top + length_mm + 5.5;
    sheet.label_left(left_unit, (1.0, y), s.small_digits());
    if let Some(u) = right_unit {
        sheet.label_right(u, (width - 1.0, y), s.small_digits());
    }
    sheet.into_image()
}

/// Règle en travers : centimètres en haut, pouces en bas pour `Both`.
fn horizontal(s: Scale, top: &[Tick], bottom: Option<&[Tick]>) -> image::GrayImage {
    let digits = s.digits();
    let edge = 0.5;
    let scale_height = 6.0 + 1.0 + 3.0;
    let height = if bottom.is_some() { 2.0 * (edge + scale_height) + 3.0 } else { edge + scale_height + 1.0 };
    let mut sheet = Sheet::new(s, height);
    let top = place(top, s.x);
    // Centrée sur la largeur imprimable, ligne de base comprise.
    let x0 = (i64::from(PRINT_WIDTH) - base_len(&top)) / 2;
    let label_x = |t: &Placed| (x0 + t.at) as f64 / s.x + t.width as f64 / 2.0 / s.x;
    let y0 = s.dy(edge);
    draw::fill_rect(&mut sheet.img, x0, y0, base_len(&top), BASE);
    for t in &top {
        draw::fill_rect(&mut sheet.img, x0 + t.at, y0, t.width, s.dy(t.len_mm));
        if let Some(n) = t.label {
            sheet.label(&n.to_string(), (label_x(t), edge + 6.0 + 1.0 + 1.5), digits, BLACK);
        }
    }
    if let Some(bottom) = bottom {
        let bottom = place(bottom, s.x);
        let y1 = s.dy(height - edge);
        draw::fill_rect(&mut sheet.img, x0, y1 - BASE, base_len(&bottom), BASE);
        for t in &bottom {
            let len = s.dy(t.len_mm);
            draw::fill_rect(&mut sheet.img, x0 + t.at, y1 - len, t.width, len);
            if let Some(n) = t.label {
                sheet.label(&n.to_string(), (label_x(t), height - edge - 6.0 - 1.0 - 1.5), digits, BLACK);
            }
        }
    }
    sheet.into_image()
}

#[cfg(test)]
mod tests {
    use super::super::tests::{black, tm88};
    use super::*;
    use crate::doc::Op;

    fn image(doc: &Doc) -> &image::GrayImage {
        doc.ops.iter().find_map(|op| if let Op::Image(img) = op { Some(img) } else { None }).unwrap()
    }

    /// Rangées où la graduation des centimètres est tracée, sur la colonne `x`.
    fn tick_rows(img: &image::GrayImage, x: u32) -> Vec<u32> {
        let rows: Vec<u32> = (0..img.height()).filter(|&y| black(img, x, y)).collect();
        // Une rangée par trait : le début de chaque groupe de rangées contiguës.
        rows.iter().enumerate().filter(|(i, y)| *i == 0 || rows[i - 1] + 1 != **y).map(|(_, y)| *y).collect()
    }

    #[test]
    fn vertical_ruler_ticks_are_one_centimetre_apart() {
        let s = tm88();
        let doc = build(20, Unit::Cm, Orientation::Vertical, s).unwrap();
        let img = image(&doc);
        // À 5 mm du bord, seules les graduations du centimètre passent.
        let rows = tick_rows(img, s.dx(5.0) as u32);
        assert_eq!(rows.len(), 21);
        let span = f64::from(rows[20] - rows[0]) / s.y;
        assert!((span - 200.0).abs() < 0.3, "20 cm mesurés : {span} mm");
    }

    #[test]
    fn calibration_lengthens_the_vertical_ruler() {
        let s = Scale::for_paper(512, 1010);
        let img = image(&build(10, Unit::Cm, Orientation::Vertical, s).unwrap()).clone();
        let rows = tick_rows(&img, s.dx(5.0) as u32);
        let dots = f64::from(rows[10] - rows[0]);
        assert!((dots - 100.0 * tm88().y * 1.01).abs() < 1.5);
    }

    #[test]
    fn horizontal_ruler_is_limited_to_six_centimetres() {
        let s = tm88();
        assert!(build(6, Unit::Cm, Orientation::Horizontal, s).is_ok());
        let err = build(7, Unit::Cm, Orientation::Horizontal, s).err().unwrap().to_string();
        assert!(err.contains("60 mm"), "{err}");
        assert!(build(2, Unit::Inch, Orientation::Horizontal, s).is_ok());
        assert!(build(3, Unit::Inch, Orientation::Horizontal, s).is_err());
        assert!(build(100, Unit::Cm, Orientation::Vertical, s).is_ok());
        assert!(build(101, Unit::Cm, Orientation::Vertical, s).is_err());
        assert!(build(0, Unit::Cm, Orientation::Vertical, s).is_err());
    }

    #[test]
    fn horizontal_ruler_fits_on_the_widest_and_narrowest_printers() {
        for paper in [512, 576] {
            let s = Scale::for_paper(paper, 1000);
            let img = image(&build(6, Unit::Both, Orientation::Horizontal, s).unwrap()).clone();
            let row = s.dy(0.5) as u32;
            let inked: Vec<u32> = (0..img.width()).filter(|&x| black(&img, x, row)).collect();
            let span = f64::from(inked.last().unwrap() - inked.first().unwrap() + 1) / s.x;
            assert!((span - 60.0).abs() < 0.7, "{paper} : {span} mm");
        }
    }

    /// Aucun trait ne déborde de la ligne de base, ni à gauche ni à droite (ni en haut ou en bas
    /// pour la règle verticale).
    #[test]
    fn ticks_stay_within_the_baseline() {
        let s = tm88();
        let img = image(&build(6, Unit::Both, Orientation::Horizontal, s).unwrap()).clone();
        let base_row = s.dy(0.5) as u32;
        let base: Vec<u32> = (0..img.width()).filter(|&x| black(&img, x, base_row)).collect();
        let (first, last) = (base[0], *base.last().unwrap());
        for y in base_row..base_row + s.dy(6.0) as u32 {
            for x in (0..img.width()).filter(|&x| black(&img, x, y)) {
                assert!((first..=last).contains(&x), "trait en x = {x}, base de {first} à {last}");
            }
        }
        let img = image(&build(10, Unit::Cm, Orientation::Vertical, s).unwrap()).clone();
        let rows: Vec<u32> = (0..img.height()).filter(|&y| black(&img, 0, y)).collect();
        let (top, bottom) = (rows[0], *rows.last().unwrap());
        // Les premiers points : là où sont les traits, à gauche du libellé « CM ».
        for x in 0..s.dx(0.9) as u32 {
            for y in (0..img.height()).filter(|&y| black(&img, x, y)) {
                assert!((top..=bottom).contains(&y), "trait en y = {y}, base de {top} à {bottom}");
            }
        }
    }

    #[test]
    fn inches_have_sixteenths() {
        let ticks = inch_ticks(25.4);
        assert_eq!(ticks.len(), 17);
        assert_eq!(ticks.iter().filter(|t| t.label.is_some()).count(), 2);
    }

    #[test]
    fn json_names() {
        let unit: Unit = serde_json::from_str("\"pouces\"").unwrap();
        let orientation: Orientation = serde_json::from_str("\"horizontale\"").unwrap();
        assert_eq!((unit, orientation), (Unit::Inch, Orientation::Horizontal));
    }
}
