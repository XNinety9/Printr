//! Coloriage : un mandala tracé au trait, différent à chaque numéro. Des couronnes de pétales,
//! de festons et de perles, répétées par symétrie autour du centre.

use std::f64::consts::PI;

use image::{GrayImage, Luma};
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};

use crate::doc::{Align, Doc, Style};

const SIZE: u32 = 512;
/// Demi-épaisseur du trait, en points.
const LINE: f64 = 1.3;

#[derive(Clone, Copy)]
enum Motif {
    /// Cercle centré.
    Ring { r: f64 },
    /// Pétale en amande entre deux rayons, de demi-largeur `w`.
    Petal { r0: f64, r1: f64, w: f64 },
    /// Cercle posé sur la couronne.
    Bead { r: f64, size: f64 },
    /// Demi-cercles accrochés à l'extérieur d'un anneau.
    Scallop { r: f64, size: f64 },
}

struct Layer {
    motif: Motif,
    /// Décalage d'un demi-secteur : les motifs se glissent entre ceux de la couronne précédente.
    shifted: bool,
}

/// Distance signée à un cercle (négative dedans).
fn circle(x: f64, y: f64, cx: f64, cy: f64, r: f64) -> f64 {
    ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r
}

impl Motif {
    /// Distance signée au motif, dans le repère local d'un secteur (le motif est centré sur l'axe x).
    fn distance(self, x: f64, y: f64, r: f64) -> f64 {
        match self {
            Motif::Ring { r: radius } => r - radius,
            Motif::Bead { r: radius, size } => circle(x, y, radius, 0.0, size),
            Motif::Scallop { r: radius, size } => {
                let d = circle(x, y, radius, 0.0, size);
                if r < radius { d.abs().max(radius - r) } else { d }
            }
            Motif::Petal { r0, r1, w } => {
                // Intersection de deux disques : une amande de longueur r1 - r0 et de largeur 2w.
                let half = (r1 - r0) / 2.0;
                let radius = (half * half + w * w) / (2.0 * w);
                let mid = (r0 + r1) / 2.0;
                circle(x, y, mid, -(radius - w), radius).max(circle(x, y, mid, radius - w, radius))
            }
        }
    }
}

fn design(rng: &mut StdRng) -> (usize, Vec<Layer>) {
    let folds = *[8, 10, 12, 12, 16].choose(rng).expect("symétries");
    let sector = 2.0 * PI / folds as f64;
    let mut layers = vec![
        Layer { motif: Motif::Ring { r: 14.0 }, shifted: false },
        Layer { motif: Motif::Bead { r: 0.0, size: 4.0 }, shifted: false },
    ];
    let mut r = 14.0;
    let mut shifted = false;
    while r < 215.0 {
        let band: f64 = rng.random_range(22.0..44.0_f64).min(244.0 - r);
        // Largeur d'un secteur à mi-couronne : les motifs ne débordent pas sur leurs voisins.
        let room = (r + band / 2.0) * sector / 2.0;
        match rng.random_range(0..4) {
            0 => {
                let w = room * rng.random_range(0.55..0.9);
                layers.push(Layer { motif: Motif::Petal { r0: r, r1: r + band, w }, shifted });
                if rng.random_bool(0.6) {
                    // Pétale intérieur, comme une nervure.
                    let inset = band * 0.25;
                    layers.push(Layer { motif: Motif::Petal { r0: r + inset, r1: r + band - inset, w: w * 0.45 }, shifted });
                }
            }
            1 => {
                let size = (band / 2.0).min(room * 0.9);
                layers.push(Layer { motif: Motif::Bead { r: r + band / 2.0, size }, shifted });
                layers.push(Layer { motif: Motif::Bead { r: r + band / 2.0, size: size * 0.4 }, shifted });
            }
            2 => {
                let size = room.min(band * 0.8);
                layers.push(Layer { motif: Motif::Scallop { r, size }, shifted });
                layers.push(Layer { motif: Motif::Bead { r: r + size * 0.45, size: size * 0.25 }, shifted });
            }
            _ => {
                // Pétales longs et fins, deux par secteur.
                let w = room * 0.35;
                layers.push(Layer { motif: Motif::Petal { r0: r, r1: r + band, w }, shifted: false });
                layers.push(Layer { motif: Motif::Petal { r0: r, r1: r + band * 0.7, w: w * 0.8 }, shifted: true });
            }
        }
        r += band;
        if rng.random_bool(0.55) {
            layers.push(Layer { motif: Motif::Ring { r }, shifted: false });
            r += rng.random_range(0.0..8.0);
        }
        shifted = !shifted;
    }
    layers.push(Layer { motif: Motif::Ring { r: 246.0 }, shifted: false });
    (folds, layers)
}

fn render(folds: usize, layers: &[Layer]) -> GrayImage {
    let sector = 2.0 * PI / folds as f64;
    let center = SIZE as f64 / 2.0;
    GrayImage::from_fn(SIZE, SIZE, |px, py| {
        let (dx, dy) = (px as f64 + 0.5 - center, py as f64 + 0.5 - center);
        let r = (dx * dx + dy * dy).sqrt();
        if r > 250.0 {
            return Luma([255]);
        }
        let theta = dy.atan2(dx);
        let ink = layers.iter().any(|layer| {
            let phase = if layer.shifted { sector / 2.0 } else { 0.0 };
            let a = (theta - phase).rem_euclid(sector);
            let a = if a > sector / 2.0 { a - sector } else { a };
            let (x, y) = (r * a.cos(), r * a.sin());
            let d = layer.motif.distance(x, y, r);
            // Petites perles pleines, contours partout ailleurs.
            match layer.motif {
                Motif::Bead { size, .. } if size < 4.5 => d < 0.5,
                _ => d.abs() < LINE,
            }
        });
        Luma([if ink { 0 } else { 255 }])
    })
}

pub fn build(seed: Option<u64>) -> Doc {
    let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
    let (folds, layers) = design(&mut StdRng::seed_from_u64(seed));

    let mut doc = Doc::new();
    doc.header("Coloriage");
    doc.feed(1);
    doc.image(render(folds, &layers));
    doc.text(&format!("n° {seed}"), Style::default().small().align(Align::Right));
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mandalas_are_line_art() {
        for seed in [1, 2, 3, 42] {
            let (folds, layers) = design(&mut StdRng::seed_from_u64(seed));
            let img = render(folds, &layers);
            let black = img.pixels().filter(|p| p.0[0] == 0).count() as f64 / (SIZE * SIZE) as f64;
            // Assez de traits pour colorier, mais pas une tache noire.
            assert!((0.04..0.3).contains(&black), "graine {seed} : {:.0} % de noir", black * 100.0);
        }
    }
}
