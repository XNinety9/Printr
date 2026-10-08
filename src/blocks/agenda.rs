//! Agenda du jour depuis un ou plusieurs calendriers ICS (lien iCloud, Google, Nextcloud… ou fichier).
//! Gère les événements sur la journée, les fuseaux horaires et les répétitions (RRULE, EXDATE,
//! occurrences déplacées).

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use rrule::{RRule, RRuleSet, Tz, Unvalidated};

use super::Ctx;
use crate::doc::{Doc, Style};
use crate::fr;

/// Un instant d'un calendrier : une date seule (événement sur la journée) ou une heure précise.
#[derive(Clone, Copy, Debug)]
enum Stamp {
    Date(NaiveDate),
    Time(DateTime<Tz>),
}

impl Stamp {
    /// Début de l'instant, en heure locale.
    fn local(self) -> NaiveDateTime {
        match self {
            Stamp::Date(d) => d.and_time(NaiveTime::MIN),
            Stamp::Time(t) => t.with_timezone(&Local).naive_local(),
        }
    }

    fn with_tz(self) -> DateTime<Tz> {
        match self {
            Stamp::Date(d) => at(Tz::LOCAL, d.and_time(NaiveTime::MIN)),
            Stamp::Time(t) => t,
        }
    }
}

fn at(tz: Tz, naive: NaiveDateTime) -> DateTime<Tz> {
    tz.from_local_datetime(&naive).earliest().unwrap_or_else(|| tz.from_utc_datetime(&naive))
}

#[derive(Default)]
struct Event {
    uid: String,
    summary: String,
    location: String,
    start: Option<Stamp>,
    end: Option<Stamp>,
    rrule: Option<String>,
    exdates: Vec<Stamp>,
    recurrence_id: Option<Stamp>,
    cancelled: bool,
}

/// Une occurrence à afficher.
#[derive(Debug, PartialEq)]
struct Item {
    start: NaiveDateTime,
    end: NaiveDateTime,
    all_day: bool,
    summary: String,
    location: String,
}

/// Recolle les lignes repliées (une ligne qui commence par un espace prolonge la précédente).
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        match line.strip_prefix([' ', '\t']) {
            Some(rest) if !lines.is_empty() => lines.last_mut().expect("ligne précédente").push_str(rest),
            _ => lines.push(line.to_owned()),
        }
    }
    lines
}

/// « DTSTART;TZID=Europe/Paris:20261008T090000 » → (nom, paramètres, valeur).
fn property(line: &str) -> Option<(String, HashMap<String, String>, String)> {
    let mut quoted = false;
    let split = line.char_indices().find(|&(_, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        c == ':' && !quoted
    })?;
    let (head, value) = (&line[..split.0], &line[split.0 + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.to_ascii_uppercase();
    let params = parts
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_owned()))
        .collect();
    Some((name, params, value.to_owned()))
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push(' '),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_owned()
}

fn stamp(value: &str, params: &HashMap<String, String>) -> Option<Stamp> {
    let value = value.trim();
    if params.get("VALUE").is_some_and(|v| v == "DATE") || value.len() == 8 {
        return NaiveDate::parse_from_str(value, "%Y%m%d").ok().map(Stamp::Date);
    }
    let (value, utc) = match value.strip_suffix('Z') {
        Some(v) => (v, true),
        None => (value, false),
    };
    let naive = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let tz = if utc {
        Tz::UTC
    } else {
        // Un fuseau inconnu (noms Windows, par exemple) est lu comme l'heure locale.
        params.get("TZID").and_then(|id| id.parse::<chrono_tz::Tz>().ok()).map(Tz::from).unwrap_or(Tz::LOCAL)
    };
    Some(Stamp::Time(at(tz, naive)))
}

fn parse(text: &str) -> Vec<Event> {
    let mut events = Vec::new();
    let mut current: Option<Event> = None;
    for line in unfold(text) {
        let Some((name, params, value)) = property(&line) else { continue };
        match (name.as_str(), current.as_mut()) {
            ("BEGIN", None) if value.eq_ignore_ascii_case("VEVENT") => current = Some(Event::default()),
            ("END", Some(_)) if value.eq_ignore_ascii_case("VEVENT") => events.extend(current.take()),
            ("UID", Some(e)) => e.uid = value,
            ("SUMMARY", Some(e)) => e.summary = unescape(&value),
            ("LOCATION", Some(e)) => e.location = unescape(&value),
            ("DTSTART", Some(e)) => e.start = stamp(&value, &params),
            ("DTEND", Some(e)) => e.end = stamp(&value, &params),
            ("RRULE", Some(e)) => e.rrule = Some(value),
            ("EXDATE", Some(e)) => e.exdates.extend(value.split(',').filter_map(|v| stamp(v, &params))),
            ("RECURRENCE-ID", Some(e)) => e.recurrence_id = stamp(&value, &params),
            ("STATUS", Some(e)) => e.cancelled = value.eq_ignore_ascii_case("CANCELLED"),
            _ => {}
        }
    }
    events
}

/// Occurrences qui chevauchent [from, to[, en heure locale.
fn occurrences(events: &[Event], from: NaiveDateTime, to: NaiveDateTime) -> Vec<Item> {
    // Occurrences déplacées ou annulées : on retire l'occurrence d'origine de la série.
    let mut moved: HashMap<&str, Vec<NaiveDateTime>> = HashMap::new();
    for e in events {
        if let Some(id) = e.recurrence_id {
            moved.entry(e.uid.as_str()).or_default().push(id.local());
        }
    }

    let mut items = Vec::new();
    for e in events.iter().filter(|e| !e.cancelled) {
        let Some(start) = e.start else { continue };
        let all_day = matches!(start, Stamp::Date(_));
        let length = match (e.end, all_day) {
            (Some(end), _) => end.local() - start.local(),
            (None, true) => Duration::days(1),
            (None, false) => Duration::zero(),
        };
        let mut starts = vec![start.local()];
        if let (Some(rule), None) = (&e.rrule, e.recurrence_id) {
            let dtstart = start.with_tz();
            let set = rule.parse::<RRule<Unvalidated>>().ok().and_then(|r| r.build(dtstart).ok());
            if let Some(mut set) = set {
                for ex in &e.exdates {
                    set = set.exdate(ex.with_tz());
                }
                let window_start = at(Tz::LOCAL, from - length - Duration::seconds(1));
                let set: RRuleSet = set.after(window_start).before(at(Tz::LOCAL, to));
                starts = set.all(500).dates.iter().map(|d| d.with_timezone(&Local).naive_local()).collect();
                let skipped = moved.get(e.uid.as_str());
                starts.retain(|s| !skipped.is_some_and(|m| m.contains(s)));
            }
        }
        for s in starts {
            let end = s + length;
            let overlaps = s < to && (end > from || (length.is_zero() && s >= from));
            if overlaps {
                items.push(Item { start: s, end, all_day, summary: e.summary.clone(), location: e.location.clone() });
            }
        }
    }
    items.sort_by(|a, b| (!a.all_day, a.start, &a.summary).cmp(&(!b.all_day, b.start, &b.summary)));
    items
}

fn fetch(ctx: &Ctx, source: &str) -> Result<String> {
    let url = match source.strip_prefix("webcal://") {
        Some(rest) => format!("https://{rest}"),
        None => source.to_owned(),
    };
    if url.starts_with("http://") || url.starts_with("https://") {
        let mut response = ctx.http.get(&url).call().context("calendrier injoignable")?;
        Ok(response.body_mut().with_config().limit(10 * 1024 * 1024).read_to_string()?)
    } else {
        std::fs::read_to_string(&url).with_context(|| format!("calendrier illisible : {url}"))
    }
}

fn day_title(day: NaiveDate, today: NaiveDate) -> String {
    let date = format!("{} {}", fr::weekday(day), fr::day_month(day));
    match (day - today).num_days() {
        0 => format!("Aujourd'hui, {date}"),
        1 => format!("Demain, {date}"),
        _ => fr::capitalize(&date),
    }
}

pub fn build(ctx: &Ctx, calendars: &[String], days: u8, title: Option<&str>) -> Result<Doc> {
    anyhow::ensure!(!calendars.is_empty(), "indiquer au moins un calendrier (`calendars`)");
    let mut events = Vec::new();
    for source in calendars {
        events.extend(parse(&fetch(ctx, source)?));
    }

    let mut doc = Doc::new();
    doc.header(title.unwrap_or("Agenda"));
    for k in 0..days.clamp(1, 7) as i64 {
        let day = ctx.today + Duration::days(k);
        let from = day.and_time(NaiveTime::MIN);
        let items = occurrences(&events, from, from + Duration::days(1));
        doc.feed(1);
        doc.text(&day_title(day, ctx.today), Style::default().bold());
        if items.is_empty() {
            doc.text("Rien de prévu.", Style::default().small());
        }
        for item in items {
            let when = if item.all_day || (item.start <= from && item.end >= from + Duration::days(1)) {
                "Journée".to_owned()
            } else {
                item.start.format("%H:%M").to_string()
            };
            doc.hanging(&format!("{when:<9}"), &item.summary, Style::default());
            if !item.location.is_empty() {
                // 12 caractères en petite police = 9 en police normale : le lieu s'aligne sous le titre.
                doc.hanging(&" ".repeat(12), &item.location, Style::default().small());
            }
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
UID:dentiste\r
SUMMARY:Dentiste\\, Léo\r
LOCATION:Cabinet du\r
  Dr Martin\r
DTSTART;TZID=Europe/Paris:20261008T093000\r
DTEND;TZID=Europe/Paris:20261008T100000\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:judo\r
SUMMARY:Judo\r
DTSTART;TZID=Europe/Paris:20260916T170000\r
DTEND;TZID=Europe/Paris:20260916T180000\r
RRULE:FREQ=WEEKLY;BYDAY=WE,TH\r
EXDATE;TZID=Europe/Paris:20261015T170000\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:judo\r
RECURRENCE-ID;TZID=Europe/Paris:20261008T170000\r
SUMMARY:Judo (gala)\r
DTSTART;TZID=Europe/Paris:20261008T150000\r
DTEND;TZID=Europe/Paris:20261008T160000\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:vacances\r
SUMMARY:Vacances\r
DTSTART;VALUE=DATE:20261017\r
DTEND;VALUE=DATE:20261102\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:annule\r
SUMMARY:Réunion annulée\r
STATUS:CANCELLED\r
DTSTART:20261008T080000Z\r
END:VEVENT\r
END:VCALENDAR\r
";

    fn day(d: u32) -> Vec<String> {
        let from = NaiveDate::from_ymd_opt(2026, 10, d).unwrap().and_time(NaiveTime::MIN);
        occurrences(&parse(SAMPLE), from, from + Duration::days(1))
            .into_iter()
            .map(|i| if i.all_day { i.summary } else { format!("{} {}", i.start.format("%H:%M"), i.summary) })
            .collect()
    }

    /// Heure de Paris, affichée dans le fuseau de la machine qui lance les tests.
    fn paris(d: u32, h: u32, m: u32) -> String {
        let naive = NaiveDate::from_ymd_opt(2026, 10, d).unwrap().and_hms_opt(h, m, 0).unwrap();
        let t = chrono_tz::Europe::Paris.from_local_datetime(&naive).unwrap();
        t.with_timezone(&Local).format("%H:%M").to_string()
    }

    #[test]
    fn reads_events_repeats_and_exceptions() {
        assert_eq!(day(8), [format!("{} Dentiste, Léo", paris(8, 9, 30)), format!("{} Judo (gala)", paris(8, 15, 0))]);
        assert_eq!(day(14), [format!("{} Judo", paris(14, 17, 0))]);
        assert_eq!(day(15), Vec::<String>::new());
        assert_eq!(day(20), ["Vacances"]);
        assert_eq!(day(2), Vec::<String>::new());
        assert_eq!(parse(SAMPLE)[0].location, "Cabinet du Dr Martin");
    }
}
