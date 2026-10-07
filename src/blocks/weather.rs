//! Météo via Open-Meteo (gratuit, sans clé) : géocodage puis prévisions.

use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::doc::{Doc, Style};
use crate::fr;

#[derive(Deserialize)]
struct GeoResults {
    results: Option<Vec<Place>>,
}

#[derive(Deserialize)]
pub struct Place {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Deserialize)]
struct Forecast {
    current: Current,
    daily: Daily,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f64,
    apparent_temperature: f64,
    weather_code: u8,
    wind_speed_10m: f64,
}

#[derive(Deserialize)]
struct Daily {
    time: Vec<NaiveDate>,
    weather_code: Vec<u8>,
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
    precipitation_probability_max: Vec<Option<f64>>,
    sunrise: Vec<String>,
    sunset: Vec<String>,
    uv_index_max: Vec<Option<f64>>,
}

/// Libellé français d'un code météo WMO.
fn describe(code: u8) -> &'static str {
    match code {
        0 => "Ciel dégagé",
        1 => "Plutôt dégagé",
        2 => "Partiellement nuageux",
        3 => "Couvert",
        45 => "Brouillard",
        48 => "Brouillard givrant",
        51 => "Bruine légère",
        53 => "Bruine",
        55 => "Bruine dense",
        56 | 57 => "Bruine verglaçante",
        61 => "Pluie faible",
        63 => "Pluie",
        65 => "Forte pluie",
        66 | 67 => "Pluie verglaçante",
        71 => "Neige faible",
        73 => "Neige",
        75 => "Forte neige",
        77 => "Grains de neige",
        80 => "Averses faibles",
        81 => "Averses",
        82 => "Fortes averses",
        85 => "Averses de neige",
        86 => "Fortes averses de neige",
        95 => "Orage",
        96 | 99 => "Orage et grêle",
        _ => "Temps indéterminé",
    }
}

/// « Lyon », ou des coordonnées « 45.76,4.83 ».
pub fn locate(agent: &ureq::Agent, location: &str) -> Result<Place> {
    if let Some((lat, lon)) = location.split_once(',') {
        if let (Ok(latitude), Ok(longitude)) = (lat.trim().parse(), lon.trim().parse()) {
            return Ok(Place { name: location.to_owned(), latitude, longitude });
        }
    }
    let geo: GeoResults = agent
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query("name", location)
        .query("count", "1")
        .query("language", "fr")
        .call()
        .context("géocodage injoignable")?
        .body_mut()
        .read_json()?;
    match geo.results.and_then(|r| r.into_iter().next()) {
        Some(place) => Ok(place),
        None => bail!("lieu introuvable : {location}"),
    }
}

/// Heure « HH:MM » d'un horodatage ISO « 2026-10-07T07:52 ».
fn hour(iso: &str) -> &str {
    iso.split_once('T').map_or(iso, |(_, t)| t)
}

pub fn build(agent: &ureq::Agent, location: &str, days: u8) -> Result<Doc> {
    let days = days.clamp(1, 7);
    let place = locate(agent, location)?;
    let f: Forecast = agent
        .get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", place.latitude.to_string())
        .query("longitude", place.longitude.to_string())
        .query("current", "temperature_2m,apparent_temperature,weather_code,wind_speed_10m")
        .query(
            "daily",
            "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,\
             sunrise,sunset,uv_index_max",
        )
        .query("timezone", "auto")
        .query("forecast_days", days.to_string())
        .call()
        .context("Open-Meteo injoignable")?
        .body_mut()
        .read_json()?;
    let (c, d) = (&f.current, &f.daily);
    let pct = |v: Option<f64>| v.map_or("-".to_owned(), |p| format!("{p:.0}%"));

    let mut doc = Doc::new();
    doc.header(&format!("Météo · {}", place.name));
    doc.text(describe(c.weather_code), Style::default().bold().center());
    doc.text(
        &format!("{:.0}°C (ressenti {:.0}°C)", c.temperature_2m, c.apparent_temperature),
        Style::default().size(2).center(),
    );
    doc.feed(1);
    doc.text(
        &format!(
            "Min {:.0}°C · Max {:.0}°C · Pluie {}",
            d.temperature_2m_min[0],
            d.temperature_2m_max[0],
            pct(d.precipitation_probability_max[0])
        ),
        Style::default().center(),
    );
    let uv = d.uv_index_max[0].map_or("-".to_owned(), |u| format!("{u:.0}"));
    doc.text(&format!("Vent {:.0} km/h · UV {uv}", c.wind_speed_10m), Style::default().center());
    doc.text(
        &format!("Soleil {} - {}", hour(&d.sunrise[0]), hour(&d.sunset[0])),
        Style::default().center(),
    );

    if d.time.len() > 1 {
        doc.feed(1);
        for i in 1..d.time.len() {
            let date = d.time[i];
            let line = format!(
                "{:<9}{:<24.24}{:>3}/{:>3}° {:>4}",
                format!("{} {}", fr::weekday_short(date), date.format("%d")),
                describe(d.weather_code[i]),
                format!("{:.0}", d.temperature_2m_min[i]),
                format!("{:.0}", d.temperature_2m_max[i]),
                pct(d.precipitation_probability_max[i]),
            );
            doc.line(&line, Style::default().small());
        }
    }
    Ok(doc)
}
