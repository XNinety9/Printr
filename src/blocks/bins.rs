//! Poubelles : « Ce soir, on sort le bac jaune », d'après des règles de ramassage simples
//! (jours de la semaine, toutes les N semaines). Rien n'est imprimé les soirs sans ramassage,
//! sauf avec `always`.

use chrono::{Datelike, Duration, NaiveDate};
use image::GrayImage;
use serde::Deserialize;

use crate::doc::{Doc, Style};
use crate::{draw, fr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Day {
    #[serde(alias = "lundi")]
    Monday,
    #[serde(alias = "mardi")]
    Tuesday,
    #[serde(alias = "mercredi")]
    Wednesday,
    #[serde(alias = "jeudi")]
    Thursday,
    #[serde(alias = "vendredi")]
    Friday,
    #[serde(alias = "samedi")]
    Saturday,
    #[serde(alias = "dimanche")]
    Sunday,
}

impl Day {
    fn number(self) -> u32 {
        self as u32
    }
}

fn one() -> u8 {
    1
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    /// « bac jaune », « verre », « ordures ménagères »…
    pub name: String,
    pub days: Vec<Day>,
    /// Toutes les N semaines (1 par défaut).
    #[serde(default = "one")]
    pub every: u8,
    /// Une date de ramassage connue, pour caler les semaines quand `every` > 1.
    #[serde(default)]
    pub from: Option<NaiveDate>,
}

/// Rappel la veille au soir (défaut) ou le jour même.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    #[default]
    #[serde(alias = "eve")]
    Veille,
    #[serde(alias = "day")]
    Jour,
}

impl Collection {
    fn collects_on(&self, date: NaiveDate) -> bool {
        if !self.days.iter().any(|d| d.number() == date.weekday().num_days_from_monday()) {
            return false;
        }
        let every = self.every.max(1) as i64;
        let Some(from) = self.from else { return every == 1 };
        // Semaines écoulées entre les lundis des deux dates.
        let monday = |d: NaiveDate| d - Duration::days(d.weekday().num_days_from_monday() as i64);
        ((monday(date) - monday(from)).num_days() / 7).rem_euclid(every) == 0
    }
}

/// Petite poubelle dessinée : couvercle, poignée et cuve rayée.
fn bin_icon() -> GrayImage {
    let mut img = draw::canvas(78);
    let cx = 256;
    draw::fill_rect(&mut img, cx - 10, 0, 20, 4);
    draw::fill_rect(&mut img, cx - 10, 0, 4, 9);
    draw::fill_rect(&mut img, cx + 6, 0, 4, 9);
    draw::fill_rect(&mut img, cx - 30, 9, 60, 7);
    for y in 18..76 {
        // Cuve légèrement évasée vers le haut.
        let half = 26 - (y - 18) / 8;
        draw::fill_rect(&mut img, cx - half, y, 4, 1);
        draw::fill_rect(&mut img, cx + half - 4, y, 4, 1);
    }
    draw::fill_rect(&mut img, cx - 19, 74, 38, 4);
    for k in [-10, 0, 10] {
        draw::fill_rect(&mut img, cx + k - 1, 26, 3, 42);
    }
    img
}

pub fn build(collections: &[Collection], when: When, always: bool, today: NaiveDate) -> anyhow::Result<Doc> {
    anyhow::ensure!(!collections.is_empty(), "indiquer au moins un ramassage (`collections`)");
    let day = if when == When::Veille { today + Duration::days(1) } else { today };
    let due: Vec<&Collection> = collections.iter().filter(|c| c.collects_on(day)).collect();

    let mut doc = Doc::new();
    if due.is_empty() {
        if always {
            let nothing = if when == When::Veille { "Pas de poubelle à sortir ce soir." } else { "Pas de ramassage aujourd'hui." };
            doc.text(nothing, Style::default().small().center());
            // Le prochain ramassage, dans les quatre semaines.
            let next = (1..=28).map(|k| day + Duration::days(k)).find_map(|d| {
                let names: Vec<&str> = collections.iter().filter(|c| c.collects_on(d)).map(|c| c.name.as_str()).collect();
                (!names.is_empty()).then(|| (d, names.join(", ")))
            });
            if let Some((date, names)) = next {
                let date = if when == When::Veille { date - Duration::days(1) } else { date };
                let text = format!("Prochain : {names}, {} {}", fr::weekday(date), fr::day_month(date));
                doc.text(&text, Style::default().small().center());
            }
        }
        return Ok(doc);
    }
    doc.image(bin_icon());
    doc.feed(1);
    let intro = if when == When::Veille { "Ce soir, on sort :" } else { "Aujourd'hui, ramassage :" };
    doc.text(intro, Style::default().center());
    for c in due {
        doc.text(&fr::capitalize(&c.name), Style::default().bold().center().size(2));
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
    }

    #[test]
    fn every_other_week() {
        // Jeudi 1er octobre 2026 : bac jaune ; puis un jeudi sur deux.
        let jaune: Collection = serde_json::from_str(r#"{"name":"bac jaune","days":["jeudi"],"every":2,"from":"2026-10-01"}"#).unwrap();
        assert!(jaune.collects_on(date(1)));
        assert!(!jaune.collects_on(date(8)));
        assert!(jaune.collects_on(date(15)));
        assert!(!jaune.collects_on(date(14)));
        let ordures: Collection = serde_json::from_str(r#"{"name":"ordures","days":["lundi","vendredi"]}"#).unwrap();
        assert!(ordures.collects_on(date(5)) && ordures.collects_on(date(9)) && !ordures.collects_on(date(6)));
    }

    #[test]
    fn reminds_the_evening_before() {
        let jaune: Collection = serde_json::from_str(r#"{"name":"bac jaune","days":["jeudi"]}"#).unwrap();
        // Mercredi 7 : on sort le bac pour jeudi.
        assert!(!build(std::slice::from_ref(&jaune), When::Veille, false, date(7)).unwrap().ops.is_empty());
        assert!(build(std::slice::from_ref(&jaune), When::Veille, false, date(8)).unwrap().ops.is_empty());
        assert!(!build(std::slice::from_ref(&jaune), When::Jour, false, date(8)).unwrap().ops.is_empty());
    }
}
