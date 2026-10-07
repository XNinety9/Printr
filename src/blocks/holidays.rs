//! Jours fériés français, calculés localement (Pâques par l'algorithme de Meeus).

use chrono::{Datelike, Duration, NaiveDate};
use serde::Deserialize;

use crate::doc::{Doc, Style};
use crate::fr;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Zone {
    #[default]
    Metropole,
    /// Ajoute le Vendredi saint et la Saint-Étienne.
    AlsaceMoselle,
}

/// Dimanche de Pâques (calendrier grégorien).
fn easter(year: i32) -> NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32).expect("date de Pâques valide")
}

/// Jours fériés d'une année, triés.
pub fn of_year(year: i32, zone: Zone) -> Vec<(NaiveDate, &'static str)> {
    let d = |m, day| NaiveDate::from_ymd_opt(year, m, day).expect("date valide");
    let e = easter(year);
    let mut days = vec![
        (d(1, 1), "Jour de l'An"),
        (e + Duration::days(1), "Lundi de Pâques"),
        (d(5, 1), "Fête du Travail"),
        (d(5, 8), "Victoire 1945"),
        (e + Duration::days(39), "Ascension"),
        (e + Duration::days(50), "Lundi de Pentecôte"),
        (d(7, 14), "Fête nationale"),
        (d(8, 15), "Assomption"),
        (d(11, 1), "Toussaint"),
        (d(11, 11), "Armistice 1918"),
        (d(12, 25), "Noël"),
    ];
    if let Zone::AlsaceMoselle = zone {
        days.push((e - Duration::days(2), "Vendredi saint"));
        days.push((d(12, 26), "Saint-Étienne"));
    }
    days.sort();
    days
}

pub fn build(today: NaiveDate, zone: Zone, count: u8) -> Doc {
    let upcoming: Vec<_> = [today.year(), today.year() + 1]
        .into_iter()
        .flat_map(|y| of_year(y, zone))
        .filter(|(date, _)| *date >= today)
        .take(count.clamp(1, 12) as usize)
        .collect();

    // « dimanche 1er novembre », avec l'année seulement si ce n'est pas l'année en cours.
    let when = |date: NaiveDate| {
        let mut s = format!("{} {}", fr::weekday(date), fr::day_month(date));
        if date.year() != today.year() {
            s.push_str(&format!(" {}", date.year()));
        }
        s
    };

    let mut doc = Doc::new();
    for (i, (date, name)) in upcoming.into_iter().enumerate() {
        let days = (date - today).num_days();
        let (line, style) = match days {
            0 => (format!("Aujourd'hui c'est férié : {name} !"), Style::default().bold().center()),
            1 => (format!("Demain, {name} : jour férié !"), Style::default().bold().center()),
            _ if i == 0 => (
                format!("Prochain jour férié : {name}, {} (dans {days} jours)", when(date)),
                Style::default().center(),
            ),
            _ => (format!("{name} : {} (J-{days})", when(date)), Style::default().small().center()),
        };
        doc.text(&line, style);
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easter_dates() {
        assert_eq!(easter(2024), NaiveDate::from_ymd_opt(2024, 3, 31).unwrap());
        assert_eq!(easter(2025), NaiveDate::from_ymd_opt(2025, 4, 20).unwrap());
        assert_eq!(easter(2026), NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
    }

    #[test]
    fn holidays_2026() {
        let days = of_year(2026, Zone::Metropole);
        assert_eq!(days.len(), 11);
        assert!(days.contains(&(NaiveDate::from_ymd_opt(2026, 5, 14).unwrap(), "Ascension")));
        assert!(days.contains(&(NaiveDate::from_ymd_opt(2026, 5, 25).unwrap(), "Lundi de Pentecôte")));
        assert_eq!(of_year(2026, Zone::AlsaceMoselle).len(), 13);
    }
}

