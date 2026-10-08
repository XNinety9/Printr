//! Pictogrammes dessinés (cœur, étoile, soleil, fleur, sourire) : les emojis ne passent pas
//! dans la page de caractères de l'imprimante, alors on les imprime en image.

use std::f64::consts::PI;

use serde::Deserialize;

use crate::doc::Doc;
use crate::draw;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[serde(alias = "coeur")]
    Heart,
    #[serde(alias = "etoile")]
    Star,
    #[serde(alias = "soleil")]
    Sun,
    #[serde(alias = "fleur")]
    Flower,
    #[serde(alias = "sourire")]
    Smile,
}

impl Shape {
    pub fn label(self) -> &'static str {
        match self {
            Shape::Heart => "cœur",
            Shape::Star => "étoile",
            Shape::Sun => "soleil",
            Shape::Flower => "fleur",
            Shape::Smile => "sourire",
        }
    }

    /// Le point (x, y), dans un repère où la forme tient dans [-1, 1]², est-il noir ?
    pub(crate) fn ink(self, x: f64, y: f64) -> bool {
        let r = (x * x + y * y).sqrt();
        let angle = y.atan2(x);
        match self {
            Shape::Heart => {
                // Courbe du cœur : (x² + y² - 1)³ - x² y³ ≤ 0, mise à l'échelle et retournée.
                let (x, y) = (x * 1.25, -y * 1.25 + 0.15);
                (x * x + y * y - 1.0).powi(3) - x * x * y.powi(3) <= 0.0
            }
            Shape::Star => {
                // Polygone à 10 sommets : pointes (rayon 0,98) et creux (rayon 0,4) alternés.
                let vertex = |k: usize| {
                    let radius = if k % 2 == 0 { 0.98 } else { 0.4 };
                    let a = -PI / 2.0 + k as f64 * PI / 5.0;
                    (radius * a.cos(), radius * a.sin() + 0.08)
                };
                // Test pair-impair : le point est dedans s'il croise un nombre impair d'arêtes.
                let mut inside = false;
                for k in 0..10 {
                    let ((x1, y1), (x2, y2)) = (vertex(k), vertex((k + 1) % 10));
                    if (y1 > y) != (y2 > y) && x < x1 + (y - y1) * (x2 - x1) / (y2 - y1) {
                        inside = !inside;
                    }
                }
                inside
            }
            Shape::Sun => {
                let disc = r <= 0.5;
                let ray = (angle * 6.0).cos() > 0.55 && (0.62..=0.98).contains(&r);
                disc || ray
            }
            Shape::Flower => {
                let petals = r <= 0.55 + 0.4 * (angle * 5.0).cos().abs().powf(0.6);
                let heart = r <= 0.28;
                // Pétales pleins, cœur blanc cerclé de noir.
                petals && !(heart && r > 0.0 && r < 0.2)
            }
            Shape::Smile => {
                let ring = (0.86..=0.98).contains(&r);
                let eye = |cx: f64| ((x - cx).powi(2) + (y + 0.3).powi(2)).sqrt() <= 0.12;
                let mouth = (0.42..=0.54).contains(&r) && y > 0.12 && angle > 0.35 && angle < PI - 0.35;
                ring || eye(-0.32) || eye(0.32) || mouth
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    #[serde(alias = "petit")]
    Small,
    #[default]
    #[serde(alias = "moyen")]
    Medium,
    #[serde(alias = "grand")]
    Large,
}

impl Size {
    fn dots(self) -> u32 {
        match self {
            Size::Small => 64,
            Size::Medium => 120,
            Size::Large => 200,
        }
    }
}

pub fn build(shape: Shape, size: Size, count: u8) -> Doc {
    let side = size.dots();
    let count = (count.clamp(1, 7) as u32).min(512 / (side + 8)).max(1);
    let gap = 8;
    let total = count * side + (count - 1) * gap;
    let mut img = draw::canvas(side);
    let x0 = (512 - total) / 2;
    for k in 0..count {
        let left = x0 + k * (side + gap);
        for py in 0..side {
            for px in 0..side {
                let x = (px as f64 + 0.5) / side as f64 * 2.0 - 1.0;
                let y = (py as f64 + 0.5) / side as f64 * 2.0 - 1.0;
                if shape.ink(x, y) {
                    img.put_pixel(left + px, py, draw::BLACK);
                }
            }
        }
    }
    let mut doc = Doc::new();
    doc.image(img);
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_have_ink_and_fit() {
        for shape in [Shape::Heart, Shape::Star, Shape::Sun, Shape::Flower, Shape::Smile] {
            let mut inked = 0;
            for i in 0..40 {
                for j in 0..40 {
                    inked += u32::from(shape.ink(i as f64 / 20.0 - 1.0, j as f64 / 20.0 - 1.0));
                }
            }
            assert!(inked > 50 && inked < 1500, "{shape:?} : {inked}");
            // Les coins restent blancs.
            assert!(!shape.ink(-1.0, -1.0) && !shape.ink(1.0, 1.0));
        }
    }
}
