//! Rapporteur : un demi-cercle gradué de 0 à 180°, posé dans le sens du papier pour profiter de
//! toute sa largeur (rayon jusqu'à 60 mm).

use std::f64::consts::FRAC_PI_2;

use anyhow::Result;

use super::{Scale, Sheet, check_range};
use crate::doc::Doc;
use crate::draw::BLACK;

pub fn build(radius: f64, s: Scale) -> Result<Doc> {
    check_range(crate::i18n::t("rayon du rapporteur", "protractor radius"), radius, 30.0, super::MAX_WIDTH_MM, "mm")?;
    let x0 = 2.0;
    let top = 3.0;
    let c = (x0, top + radius);
    let mut sheet = Sheet::new(s, 2.0 * radius + 2.0 * top);
    // Base, arc extérieur, et le centre à poser sur le sommet de l'angle.
    sheet.vline(x0, top, top + 2.0 * radius, 3);
    sheet.arc(c, radius, 0.35, -FRAC_PI_2, FRAC_PI_2);
    sheet.arc(c, 2.5, 0.25, -FRAC_PI_2, FRAC_PI_2);
    sheet.line(c, (x0 + 4.0, c.1), 0.25, None);
    // 0° en haut, 90° à droite, 180° en bas.
    let at = |deg: u32, r: f64| {
        let a = f64::from(deg).to_radians();
        (c.0 + r * a.sin(), c.1 - r * a.cos())
    };
    for deg in 0..=180 {
        let (len, width) = match deg {
            d if d % 10 == 0 => (5.0, 0.35),
            d if d % 5 == 0 => (3.5, 0.25),
            _ => (2.0, 0.2),
        };
        sheet.line(at(deg, radius - len), at(deg, radius), width, None);
        if deg % 30 == 0 && deg % 180 != 0 {
            // Repères pour les angles courants : du centre jusqu'aux chiffres.
            sheet.line(at(deg, 4.0), at(deg, radius - 17.5), 0.15, None);
        }
    }
    let digits = s.small_digits();
    for deg in (0..=180).step_by(10) {
        // 0 et 180 tombent sur la base : décalés vers l'intérieur.
        let (x, y) = at(deg, radius - 8.0);
        let x = if deg % 180 == 0 { x + 3.5 } else { x };
        sheet.label(&deg.to_string(), (x, y), digits, BLACK);
        if radius >= 40.0 && deg % 180 != 0 {
            sheet.label(&(180 - deg).to_string(), at(deg, radius - 13.0), digits, BLACK);
        }
    }
    let mut doc = Doc::new();
    doc.image(sheet.into_image());
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::super::tests::tm88;
    use super::*;

    #[test]
    fn protractor_fits_the_paper() {
        for paper in [512, 576] {
            let s = Scale::for_paper(paper, 1000);
            assert!(build(60.0, s).is_ok());
        }
        assert!(build(61.0, tm88()).is_err());
        assert!(build(29.0, tm88()).is_err());
    }
}
