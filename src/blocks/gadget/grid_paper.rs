//! Papier quadrillé, ligné, pointillé ou Seyès, sur la longueur voulue.

use anyhow::Result;
use serde::Deserialize;

use super::{Scale, Sheet, check_range};
use crate::doc::Doc;
use crate::draw::BLACK;
use crate::i18n;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    #[default]
    #[serde(alias = "quadrille", alias = "carreaux")]
    Grid,
    #[serde(alias = "ligne", alias = "lignes")]
    Lines,
    #[serde(alias = "points", alias = "pointille")]
    Dots,
    /// Grands carreaux d'écolier : lignes tous les 2 mm, plus marquées tous les 8 mm.
    Seyes,
}

impl Pattern {
    pub fn label(self) -> &'static str {
        match self {
            Pattern::Grid => i18n::t("quadrillé", "grid"),
            Pattern::Lines => i18n::t("ligné", "lined"),
            Pattern::Dots => i18n::t("pointillé", "dot grid"),
            Pattern::Seyes => "Seyès",
        }
    }
}

/// Les positions régulières de `step` mm qui tiennent dans `size`, centrées.
fn steps(size: f64, step: f64) -> Vec<f64> {
    let count = (size / step + 1e-6).floor() as u32;
    let offset = (size - f64::from(count) * step) / 2.0;
    (0..=count).map(|k| offset + f64::from(k) * step).collect()
}

/// `length` en centimètres, `spacing` en mm (sans effet sur le Seyès).
pub fn build(pattern: Pattern, spacing: f64, length: u16, s: Scale) -> Result<Doc> {
    check_range(i18n::t("longueur du papier", "paper length"), f64::from(length), 2.0, 100.0, "cm")?;
    check_range(i18n::t("écart", "spacing"), spacing, 3.0, 15.0, "mm")?;
    let width = s.width_mm();
    let height = f64::from(length) * 10.0;
    let mut sheet = Sheet::new(s, height);
    match pattern {
        Pattern::Grid => {
            for x in steps(width, spacing) {
                sheet.vline(x, 0.0, height, 1);
            }
            for y in steps(height, spacing) {
                sheet.hline(y, 0.0, width, 1);
            }
        }
        Pattern::Lines => {
            for y in steps(height, spacing) {
                sheet.hline(y, 0.0, width, 1);
            }
        }
        Pattern::Dots => {
            for y in steps(height, spacing) {
                for x in steps(width, spacing) {
                    sheet.disc((x, y), 0.3, BLACK);
                }
            }
        }
        Pattern::Seyes => {
            for (k, y) in steps(height, 2.0).into_iter().enumerate() {
                sheet.hline(y, 0.0, width, if k % 4 == 0 { 2 } else { 1 });
            }
            for x in steps(width, 8.0) {
                sheet.vline(x, 0.0, height, 1);
            }
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
    use crate::doc::Op;

    #[test]
    fn paper_has_the_requested_length() {
        let s = tm88();
        let doc = build(Pattern::Seyes, 5.0, 10, s).unwrap();
        let Op::Image(img) = &doc.ops[0] else { panic!() };
        assert!((f64::from(img.height()) / s.y - 100.0).abs() < 0.2);
        assert!(build(Pattern::Grid, 2.0, 10, s).is_err());
        assert!(build(Pattern::Grid, 5.0, 101, s).is_err());
    }

    #[test]
    fn steps_are_centered() {
        let xs = steps(72.0, 5.0);
        assert_eq!(xs.len(), 15);
        assert!((xs[0] - 1.0).abs() < 1e-9 && (xs[14] - 71.0).abs() < 1e-9);
    }
}
