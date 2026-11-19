//! Formes à la taille exacte, à découper ou à tracer : cercle, carré et rectangle.

use anyhow::Result;

use super::{Dash, Scale, Sheet, check_range, fmt_mm};
use crate::doc::{Doc, Style};
use crate::i18n;

/// Épaisseur du trait, en mm : on découpe au milieu.
const LINE: f64 = 0.35;

pub fn circle(diameter: f64, dashed: bool, center: bool, label: bool, s: Scale) -> Result<Doc> {
    check_range(i18n::t("diamètre du cercle", "circle diameter"), diameter, 5.0, super::MAX_WIDTH_MM, "mm")?;
    let r = diameter / 2.0;
    let c = (s.width_mm() / 2.0, 1.0 + r);
    let mut sheet = Sheet::new(s, diameter + 2.0);
    sheet.circle(c, r, LINE, dashed.then_some(Dash::CUT));
    if center {
        sheet.cross(c, (diameter / 4.0).min(4.0));
    }
    let mut doc = Doc::new();
    doc.image(sheet.into_image());
    if label {
        doc.text(&format!("Ø {} mm", fmt_mm(diameter)), Style::default().small().center());
    }
    Ok(doc)
}

pub fn rectangle(width: f64, height: f64, dashed: bool, label: bool, s: Scale) -> Result<Doc> {
    check_range(i18n::t("largeur du rectangle", "rectangle width"), width, 5.0, super::MAX_WIDTH_MM, "mm")?;
    check_range(i18n::t("hauteur du rectangle", "rectangle height"), height, 5.0, 300.0, "mm")?;
    let x0 = (s.width_mm() - width) / 2.0;
    let y0 = 1.0;
    let mut sheet = Sheet::new(s, height + 2.0);
    let corners = [(x0, y0), (x0 + width, y0), (x0 + width, y0 + height), (x0, y0 + height)];
    sheet.polygon(&corners, LINE, dashed.then_some(Dash::CUT));
    let mut doc = Doc::new();
    doc.image(sheet.into_image());
    if label {
        doc.text(&format!("{} × {} mm", fmt_mm(width), fmt_mm(height)), Style::default().small().center());
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{black, tm88};
    use super::*;
    use crate::doc::Op;

    #[test]
    fn rectangle_has_the_requested_size() {
        let s = tm88();
        let doc = rectangle(40.0, 25.0, false, true, s).unwrap();
        let Op::Image(img) = &doc.ops[0] else { panic!() };
        let column = img.width() / 2;
        let rows: Vec<u32> = (0..img.height()).filter(|&y| black(img, column, y)).collect();
        let span = f64::from(rows.last().unwrap() - rows.first().unwrap()) / s.y;
        assert!((span - 25.0).abs() < 0.6, "{span} mm");
        assert!(matches!(&doc.ops[1], Op::Line { text, .. } if text.contains("40 × 25 mm")));
    }

    #[test]
    fn shapes_limits() {
        let s = tm88();
        assert!(circle(61.0, false, false, false, s).is_err());
        assert!(circle(4.0, false, false, false, s).is_err());
        assert!(rectangle(60.0, 300.0, true, false, s).is_ok());
        assert!(rectangle(60.0, 301.0, true, false, s).is_err());
    }
}
