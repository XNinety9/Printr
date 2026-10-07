//! Phase de la lune, calculée localement à partir d'une nouvelle lune de référence.
//! Précision de l'ordre d'une demi-journée : largement suffisant pour un ticket.

use std::f64::consts::TAU;

use chrono::{DateTime, Duration, Local, TimeZone, Utc};

use crate::doc::{Doc, Style};
use crate::{draw, fr};

/// Durée moyenne d'une lunaison, en jours.
const SYNODIC_MONTH: f64 = 29.530_588_853;

/// Âge de la lune (en jours depuis la dernière nouvelle lune).
fn age(now: DateTime<Utc>) -> f64 {
    let reference = Utc.with_ymd_and_hms(2000, 1, 6, 18, 14, 0).unwrap();
    let days = (now - reference).num_seconds() as f64 / 86_400.0;
    days.rem_euclid(SYNODIC_MONTH)
}

fn name(age: f64) -> &'static str {
    const NAMES: [&str; 8] = [
        "Nouvelle lune",
        "Premier croissant",
        "Premier quartier",
        "Gibbeuse croissante",
        "Pleine lune",
        "Gibbeuse décroissante",
        "Dernier quartier",
        "Dernier croissant",
    ];
    let octant = ((age / SYNODIC_MONTH * 8.0) + 0.5).floor() as usize % 8;
    NAMES[octant]
}

const DIAMETER: i64 = 150;

/// Disque de la lune : partie éclairée en blanc, partie sombre en gris tramé.
fn render(fraction: f64) -> image::GrayImage {
    let r = DIAMETER as f64 / 2.0;
    let mut img = draw::canvas(DIAMETER as u32 + 4);
    let (cx, cy) = (256.0, r + 2.0);
    let k = (TAU * fraction).cos();
    for py in 0..img.height() {
        for px in 0..img.width() {
            let (u, v) = ((px as f64 - cx) / r, (py as f64 - cy) / r);
            let d = (u * u + v * v).sqrt();
            if d > 1.0 + 3.0 / r {
                continue;
            }
            if d > 1.0 - 1.5 / r {
                img.put_pixel(px, py, draw::BLACK); // contour
                continue;
            }
            let w = (1.0 - v * v).sqrt();
            // Hémisphère nord : la lune croissante est éclairée à droite.
            let lit = if fraction < 0.5 { u > k * w } else { u < -k * w };
            if !lit && (px + py) % 2 == 0 {
                img.put_pixel(px, py, draw::BLACK);
            }
        }
    }
    img
}

pub fn build(now: DateTime<Utc>) -> Doc {
    let age = age(now);
    let fraction = age / SYNODIC_MONTH;
    let illumination = (1.0 - (TAU * fraction).cos()) / 2.0 * 100.0;
    let in_days = |target: f64| (target - age).rem_euclid(SYNODIC_MONTH);
    let date_in = |days: f64| {
        let when = (now + Duration::seconds((days * 86_400.0) as i64)).with_timezone(&Local);
        format!("{} {}", fr::weekday_short(when.date_naive()), fr::day_month(when.date_naive()))
    };

    let mut doc = Doc::new();
    doc.header("Lune");
    doc.feed(1);
    doc.image(render(fraction));
    doc.text(name(age), Style::default().bold().center());
    doc.text(&format!("Éclairée à {illumination:.0} %"), Style::default().center());
    let (full, new) = (in_days(SYNODIC_MONTH / 2.0), in_days(0.0));
    let mut next = [(full, "Pleine lune"), (new, "Nouvelle lune")];
    next.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (days, label) in next {
        doc.text(&format!("{label} : {}", date_in(days)), Style::default().small().center());
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_phases() {
        // Pleine lune du 25 décembre 2015 à 11 h 11 UTC, nouvelle lune du 21 août 2017 (éclipse).
        let full = Utc.with_ymd_and_hms(2015, 12, 25, 11, 11, 0).unwrap();
        assert_eq!(name(age(full)), "Pleine lune");
        let new = Utc.with_ymd_and_hms(2017, 8, 21, 18, 30, 0).unwrap();
        assert_eq!(name(age(new)), "Nouvelle lune");
    }
}
