//! Cible à anneaux, pour les fléchettes, le tir à l'élastique ou les pistolets à mousse.

use anyhow::Result;

use super::{Scale, Sheet, check_range, opposite};
use crate::doc::Doc;
use crate::draw::{BLACK, WHITE};

pub struct Options {
    /// Diamètre, en mm.
    pub diameter: f64,
    pub rings: u8,
    /// Anneaux noirs et blancs en alternance, plutôt que de simples cercles.
    pub filled: bool,
    /// Points de chaque anneau, du plus grand au centre.
    pub numbers: bool,
    /// Viseur : une croix sur toute la cible.
    pub crosshair: bool,
}

pub fn build(o: &Options, s: Scale) -> Result<Doc> {
    check_range(
        crate::i18n::t("diamètre de la cible", "target diameter"),
        o.diameter,
        20.0,
        super::MAX_WIDTH_MM,
        "mm",
    )?;
    check_range(crate::i18n::t("anneaux", "rings"), f64::from(o.rings), 2.0, 10.0, "")?;
    let r = o.diameter / 2.0;
    let n = f64::from(o.rings);
    let center = (s.width_mm() / 2.0, 1.0 + r);
    let mut sheet = Sheet::new(s, o.diameter + 2.0);
    // Anneau k (1 au centre) : rayon extérieur r·k/n, noir un sur deux en partant du centre.
    let ink = |k: u8| if o.filled && k % 2 == 1 { BLACK } else { WHITE };
    for k in (1..=o.rings).rev() {
        sheet.disc(center, r * f64::from(k) / n, ink(k));
    }
    for k in 1..=o.rings {
        sheet.circle(center, r * f64::from(k) / n, 0.35, None);
    }
    if o.crosshair {
        let (cx, cy) = center;
        sheet.invert_where((cx - r, cy - r, cx + r, cy + r), |x, y| {
            (x - cx).hypot(y - cy) <= r && ((x - cx).abs() <= 0.15 || (y - cy).abs() <= 0.15)
        });
    } else if !o.filled && !o.numbers {
        sheet.disc(center, 0.6, BLACK);
    }
    // Les points, en diagonale (le viseur passe à l'horizontale et à la verticale), si l'anneau
    // est assez large pour les chiffres.
    let band = r / n;
    let scale = if band >= 3.6 { s.digits() } else { s.small_digits() };
    if o.numbers && band >= 2.6 {
        let diagonal = std::f64::consts::FRAC_1_SQRT_2;
        for k in 1..=o.rings {
            let points = o.rings - k + 1;
            let mid = match k {
                1 if !o.crosshair => 0.0,
                // Au centre, juste à côté de la croix du viseur.
                1 => band * 0.7,
                _ => band * (f64::from(k) - 0.5),
            };
            let at = (center.0 + mid * diagonal, center.1 - mid * diagonal);
            sheet.label(&points.to_string(), at, scale, opposite(ink(k)));
        }
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
        Options { diameter: 60.0, rings: 5, filled: true, numbers: true, crosshair: true }
    }

    #[test]
    fn target_has_the_requested_diameter() {
        let s = tm88();
        let doc = build(&options(), s).unwrap();
        let Op::Image(img) = &doc.ops[0] else { panic!() };
        let row = s.dy(31.0) as u32 + 3;
        let inked: Vec<u32> = (0..img.width()).filter(|&x| black(img, x, row)).collect();
        let span = f64::from(inked.last().unwrap() - inked.first().unwrap()) / s.x;
        assert!((span - 60.0).abs() < 1.0, "{span} mm");
    }

    #[test]
    fn target_limits() {
        let s = tm88();
        assert!(build(&Options { diameter: 61.0, ..options() }, s).is_err());
        assert!(build(&Options { rings: 11, ..options() }, s).is_err());
        assert!(build(&Options { diameter: 20.0, rings: 10, ..options() }, s).is_ok());
    }
}
