//! Dates en français.

use chrono::{Datelike, NaiveDate};

const DAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const MONTHS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
    "novembre", "décembre",
];

pub fn weekday(date: NaiveDate) -> &'static str {
    DAYS[date.weekday().num_days_from_monday() as usize]
}

/// « mar. », « mer. »…
pub fn weekday_short(date: NaiveDate) -> String {
    format!("{}.", &weekday(date)[..3])
}

pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// « 1er octobre », « 7 octobre ».
pub fn day_month(date: NaiveDate) -> String {
    let day = match date.day() {
        1 => "1er".to_owned(),
        d => d.to_string(),
    };
    format!("{day} {}", MONTHS[date.month0() as usize])
}

/// « mardi 7 octobre 2026 ».
pub fn long_date(date: NaiveDate) -> String {
    format!("{} {} {}", weekday(date), day_month(date), date.year())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        assert_eq!(long_date(d), "mercredi 7 octobre 2026");
        assert_eq!(weekday_short(d), "mer.");
        let d = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        assert_eq!(capitalize(&long_date(d)), "Samedi 1er août 2026");
    }
}
