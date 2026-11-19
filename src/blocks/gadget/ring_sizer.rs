//! Baguier : une bande à enrouler autour du doigt pour trouver sa taille de bague (taille
//! française et européenne : le tour de doigt en millimètres).

use anyhow::Result;

use super::{Dash, Scale, Sheet};
use crate::doc::{Doc, Style};
use crate::i18n;

/// Tailles graduées, en mm de tour de doigt.
const SIZES: std::ops::RangeInclusive<u32> = 40..=76;
/// Longueur de la bande, en mm.
const STRIP: f64 = 86.0;
const WIDTH: f64 = 14.0;

pub fn build(s: Scale) -> Result<Doc> {
    let top = 2.0;
    let x0 = (s.width_mm() - WIDTH) / 2.0;
    let x1 = x0 + WIDTH;
    let mut sheet = Sheet::new(s, top + STRIP + 2.0);
    // Contour à découper : le bord du haut est le zéro, donc un trait plein et net.
    sheet.hline(top, x0, x1, 2);
    let rest = [(x1, top), (x1, top + STRIP), (x0, top + STRIP), (x0, top)];
    for pair in rest.windows(2) {
        sheet.line(pair[0], pair[1], 0.3, Some(Dash::CUT));
    }
    let digits = s.small_digits();
    for mm in SIZES {
        let y = top + f64::from(mm);
        let len = if mm % 2 == 0 { 4.0 } else { 2.0 };
        sheet.hline(y, x1 - len, x1, 2);
        // Les tailles paires, sur deux colonnes pour rester lisibles.
        if mm % 2 == 0 {
            let column = if mm % 4 == 0 { 0.0 } else { 4.5 };
            sheet.label_right(&mm.to_string(), (x1 - 5.0 - column, y), digits);
            if column > 0.0 {
                sheet.hline(y, x1 - 5.0 - column + 0.3, x1 - len - 0.5, 1);
            }
        }
    }
    sheet.label("0", (x0 + WIDTH / 2.0, top + 3.0), s.digits(), crate::draw::BLACK);
    let mut doc = Doc::new();
    doc.header(i18n::t("Baguier", "Ring sizer"));
    doc.image(sheet.into_image());
    for line in [
        i18n::t("1. Découpe la bande en suivant le contour.", "1. Cut out the strip along the outline."),
        i18n::t(
            "2. Enroule-la autour du doigt, chiffres vers l'extérieur, sans serrer.",
            "2. Wrap it around the finger, numbers facing out, without squeezing.",
        ),
        i18n::t(
            "3. Le bord « 0 » tombe sur ta taille (taille française et européenne).",
            "3. The “0” edge points to your size (European size: finger circumference in mm).",
        ),
    ] {
        doc.text(line, Style::default().small());
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::super::tests::tm88;
    use super::*;

    #[test]
    fn ring_sizer_builds() {
        let doc = build(tm88()).unwrap();
        assert!(doc.ops.len() >= 4);
    }
}
