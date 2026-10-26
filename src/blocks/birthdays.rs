//! Anniversaires : « Aujourd'hui, Léo a 12 ans ! », puis le compte à rebours des suivants.
//!
//! Une personne par ligne : « Léo, 12/03/2014 », « Camille 14/11 » (année facultative, pour l'âge), ou
//! « Alex 1990-06-02 ». Jour d'abord en français, mois d'abord en anglais ; la date ISO marche partout.

use anyhow::{Result, bail};
use chrono::{Datelike, NaiveDate};

use crate::doc::{Doc, Style};
use crate::i18n;

#[derive(Clone, Debug, PartialEq)]
struct Birthday {
    name: String,
    month: u32,
    day: u32,
    year: Option<i32>,
}

/// Une ligne : le nom, puis la date (dernier mot).
fn parse_line(line: &str) -> Option<Birthday> {
    let line = line.trim();
    let (name, date) = line.rsplit_once([' ', ',', ':', '\t'])?;
    let name = name.trim().trim_end_matches([',', ':']).trim();
    if name.is_empty() {
        return None;
    }
    let (month, day, year) = if let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        (d.month(), d.day(), Some(d.year()))
    } else {
        let parts: Vec<&str> = date.split(['/', '.', '-']).collect();
        let num = |i: usize| parts.get(i).and_then(|p| p.parse::<i32>().ok());
        let (a, b) = (num(0)?, num(1)?);
        let year = match parts.len() {
            2 => None,
            3 => Some(num(2)?).map(|y| if y < 100 { y + if y > 30 { 1900 } else { 2000 } } else { y }),
            _ => return None,
        };
        let (day, month) = if i18n::en() { (b, a) } else { (a, b) };
        (month as u32, day as u32, year)
    };
    // Une date qui existe (le 29 février, dans une année bissextile).
    NaiveDate::from_ymd_opt(year.unwrap_or(2024), month, day)?;
    Some(Birthday { name: name.chars().take(40).collect(), month, day, year })
}

/// Le prochain anniversaire à partir de `today` (aujourd'hui compris) ; le 29 février tombe le 28 les
/// années ordinaires.
fn next(b: &Birthday, today: NaiveDate) -> NaiveDate {
    let on = |year: i32| {
        NaiveDate::from_ymd_opt(year, b.month, b.day)
            .or_else(|| NaiveDate::from_ymd_opt(year, b.month, b.day - 1))
            .expect("date vérifiée")
    };
    let this_year = on(today.year());
    if this_year >= today { this_year } else { on(today.year() + 1) }
}

/// « 14 nov. », « Nov 14 ».
fn short_date(date: NaiveDate) -> String {
    let month = i18n::month(date.month0());
    let short: String = month.chars().take(if month.chars().count() <= 4 { 4 } else { 3 }).collect();
    let dot = if short.chars().count() < month.chars().count() { "." } else { "" };
    if i18n::en() {
        format!("{} {}", i18n::capitalize(&short), date.day())
    } else {
        format!("{} {short}{dot}", date.day())
    }
}

fn age(b: &Birthday, on: NaiveDate) -> Option<i32> {
    b.year.map(|y| on.year() - y).filter(|a| *a > 0)
}

/// Les anniversaires du bloc puis ceux de la famille, sans doublon (même nom, même jour).
fn merge(list: &str, family: &str) -> Vec<Birthday> {
    let mut people: Vec<Birthday> = Vec::new();
    for b in list.lines().chain(family.lines()).filter_map(parse_line) {
        let same = |o: &Birthday| o.name.to_lowercase() == b.name.to_lowercase() && (o.month, o.day) == (b.month, b.day);
        if !people.iter().any(same) {
            people.push(b);
        }
    }
    people.truncate(200);
    people
}

pub fn build(list: &str, family: &str, upcoming: u8, today: NaiveDate) -> Result<Doc> {
    let people = merge(list, family);
    if people.is_empty() {
        bail!(
            "{}",
            i18n::t(
                "aucun anniversaire lisible (une ligne par personne : « Léo, 12/03/2014 »)",
                "no readable birthday (one line per person: \"Leo, 03/12/2014\")"
            )
        );
    }
    let mut dated: Vec<(NaiveDate, &Birthday)> = people.iter().map(|b| (next(b, today), b)).collect();
    dated.sort_by_key(|(d, b)| (*d, b.name.clone()));

    let mut doc = Doc::new();
    doc.header(i18n::t("Anniversaires", "Birthdays"));
    let today_ones: Vec<&Birthday> = dated.iter().filter(|(d, _)| *d == today).map(|(_, b)| *b).collect();
    for b in &today_ones {
        doc.feed(1);
        doc.text(
            &tr!("Joyeux anniversaire {} !", "Happy birthday {}!", b.name),
            Style::default().bold().center().size(2),
        );
        if let Some(a) = age(b, today) {
            doc.text(&tr!("{a} ans aujourd'hui", "{a} today"), Style::default().center());
        }
    }
    if !today_ones.is_empty() {
        doc.feed(1);
        doc.image(super::picto::image(super::picto::Shape::Heart, 64, 3));
    }
    let later: Vec<&(NaiveDate, &Birthday)> =
        dated.iter().filter(|(d, _)| *d != today).take(upcoming.clamp(1, 10) as usize).collect();
    if !later.is_empty() {
        doc.feed(1);
        doc.text(i18n::t("Les prochains", "Coming up"), Style::default().bold().underline());
        for (date, b) in later {
            let days = (*date - today).num_days();
            let when = match days {
                1 => i18n::t("demain", "tomorrow").to_owned(),
                n => tr!("J-{n}", "in {n} days"),
            };
            let who = match age(b, *date) {
                Some(a) => tr!("{}, {a} ans", "{}, turns {a}", b.name),
                None => b.name.clone(),
            };
            doc.hanging("- ", &format!("{} · {who} · {when}", short_date(*date)), Style::default());
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn lines_read_in_both_languages() {
        let fr = |l: &str| i18n::with_lang(i18n::Lang::Fr, || parse_line(l));
        let en = |l: &str| i18n::with_lang(i18n::Lang::En, || parse_line(l));
        assert_eq!(fr("Léo, 12/03/2014"), Some(Birthday { name: "Léo".into(), month: 3, day: 12, year: Some(2014) }));
        assert_eq!(
            fr("Camille Martin 14/11"),
            Some(Birthday { name: "Camille Martin".into(), month: 11, day: 14, year: None })
        );
        assert_eq!(en("Leo: 03/12/2014").map(|b| (b.month, b.day)), Some((3, 12)));
        assert_eq!(fr("Alex 1990-06-02").map(|b| (b.month, b.year)), Some((6, Some(1990))));
        assert!(fr("Personne 31/02").is_none());
        assert!(fr("12/03").is_none());
    }

    #[test]
    fn next_birthday_and_age() {
        let leo = Birthday { name: "Léo".into(), month: 3, day: 12, year: Some(2014) };
        assert_eq!(next(&leo, d(2026, 3, 12)), d(2026, 3, 12));
        assert_eq!(next(&leo, d(2026, 3, 13)), d(2027, 3, 12));
        assert_eq!(age(&leo, d(2026, 3, 12)), Some(12));
        let leap = Birthday { name: "Bissextile".into(), month: 2, day: 29, year: None };
        assert_eq!(next(&leap, d(2026, 1, 1)), d(2026, 2, 28));
    }

    #[test]
    fn birthday_of_the_day_comes_first() {
        let doc =
            i18n::with_lang(i18n::Lang::Fr, || build("Léo, 20/10/2014\nCamille 25/10", "", 3, d(2026, 10, 20)).unwrap());
        let text = doc.preview(false);
        // Le titre est en double taille : l'aperçu l'écrit en lettres espacées.
        assert!(text.contains("L é o   !"));
        assert!(text.contains("12 ans aujourd'hui"));
        assert!(text.contains("25 oct. · Camille · J-5"));
    }

    #[test]
    fn family_birthdays_join_without_duplicates() {
        let people = i18n::with_lang(i18n::Lang::Fr, || merge("Léo, 20/10/2014", "léo 20/10\nMamie, 02/01/1950"));
        let names: Vec<&str> = people.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["Léo", "Mamie"], "Léo n'est compté qu'une fois, avec son année");
        assert_eq!(people[0].year, Some(2014));
    }
}
