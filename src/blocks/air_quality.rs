//! Qualité de l'air et pollens (Europe), via Open-Meteo.

use anyhow::{Context, Result};
use serde::Deserialize;

use super::weather;
use crate::doc::{Doc, Style};

#[derive(Deserialize)]
struct Response {
    current: Current,
}

#[derive(Deserialize)]
struct Current {
    european_aqi: Option<f64>,
    pm2_5: Option<f64>,
    pm10: Option<f64>,
    ozone: Option<f64>,
    nitrogen_dioxide: Option<f64>,
    alder_pollen: Option<f64>,
    birch_pollen: Option<f64>,
    grass_pollen: Option<f64>,
    mugwort_pollen: Option<f64>,
    olive_pollen: Option<f64>,
    ragweed_pollen: Option<f64>,
}

/// Qualificatif de l'indice européen de qualité de l'air.
fn aqi_label(aqi: f64) -> &'static str {
    match aqi {
        a if a <= 20.0 => "Bonne",
        a if a <= 40.0 => "Correcte",
        a if a <= 60.0 => "Moyenne",
        a if a <= 80.0 => "Médiocre",
        a if a <= 100.0 => "Très médiocre",
        _ => "Extrêmement médiocre",
    }
}

pub fn build(agent: &ureq::Agent, location: &str) -> Result<Doc> {
    let place = weather::locate(agent, location)?;
    let r: Response = agent
        .get("https://air-quality-api.open-meteo.com/v1/air-quality")
        .query("latitude", place.latitude.to_string())
        .query("longitude", place.longitude.to_string())
        .query(
            "current",
            "european_aqi,pm10,pm2_5,ozone,nitrogen_dioxide,alder_pollen,birch_pollen,grass_pollen,\
             mugwort_pollen,olive_pollen,ragweed_pollen",
        )
        .query("timezone", "auto")
        .call()
        .context("Open-Meteo injoignable")?
        .body_mut()
        .read_json()?;
    let c = r.current;
    let v = |x: Option<f64>| x.map_or("-".to_owned(), |x| format!("{x:.0}"));

    let mut doc = Doc::new();
    doc.header(&format!("Air · {}", place.name));
    if let Some(aqi) = c.european_aqi {
        doc.text(&format!("Qualité de l'air : {}", aqi_label(aqi)), Style::default().bold().center());
        doc.text(&format!("Indice européen {aqi:.0}"), Style::default().small().center());
    }
    doc.text(
        &format!("PM2,5 {} · PM10 {} · O3 {} · NO2 {} µg/m³", v(c.pm2_5), v(c.pm10), v(c.ozone), v(c.nitrogen_dioxide)),
        Style::default().small().center(),
    );

    let pollens: Vec<String> = [
        ("Aulne", c.alder_pollen),
        ("Bouleau", c.birch_pollen),
        ("Graminées", c.grass_pollen),
        ("Armoise", c.mugwort_pollen),
        ("Olivier", c.olive_pollen),
        ("Ambroisie", c.ragweed_pollen),
    ]
    .into_iter()
    .filter_map(|(name, value)| value.filter(|&g| g >= 1.0).map(|g| format!("{name} {g:.0}")))
    .collect();
    let pollen_line = if pollens.is_empty() {
        "Pollens : rien à signaler".to_owned()
    } else {
        format!("Pollens (grains/m³) : {}", pollens.join(" · "))
    };
    doc.text(&pollen_line, Style::default().small().center());
    Ok(doc)
}
