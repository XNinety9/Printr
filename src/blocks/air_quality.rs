//! Qualité de l'air et pollens (Europe), via Open-Meteo.

use anyhow::{Context, Result};
use serde::Deserialize;

use super::weather;
use crate::doc::{Doc, Style};
use crate::i18n;

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
        a if a <= 20.0 => i18n::t("Bonne", "Good"),
        a if a <= 40.0 => i18n::t("Correcte", "Fair"),
        a if a <= 60.0 => i18n::t("Moyenne", "Moderate"),
        a if a <= 80.0 => i18n::t("Médiocre", "Poor"),
        a if a <= 100.0 => i18n::t("Très médiocre", "Very poor"),
        _ => i18n::t("Extrêmement médiocre", "Extremely poor"),
    }
}

pub fn build(agent: &ureq::Agent, location: &str, pollutants: bool, pollen: bool) -> Result<Doc> {
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
        .context(i18n::t("Open-Meteo injoignable", "Open-Meteo unreachable"))?
        .body_mut()
        .read_json()?;
    let c = r.current;
    let v = |x: Option<f64>| x.map_or("-".to_owned(), |x| format!("{x:.0}"));

    let mut doc = Doc::new();
    doc.header(&format!("Air · {}", place.name));
    if let Some(aqi) = c.european_aqi {
        doc.text(&tr!("Qualité de l'air : {}", "Air quality: {}", aqi_label(aqi)), Style::default().bold().center());
        doc.text(&tr!("Indice européen {aqi:.0}", "European AQI {aqi:.0}"), Style::default().small().center());
    }
    if pollutants {
        doc.text(
            &tr!(
                "PM2,5 {} · PM10 {} · O3 {} · NO2 {} µg/m³",
                "PM2.5 {} · PM10 {} · O3 {} · NO2 {} µg/m³",
                v(c.pm2_5),
                v(c.pm10),
                v(c.ozone),
                v(c.nitrogen_dioxide)
            ),
            Style::default().small().center(),
        );
    }
    if !pollen {
        return Ok(doc);
    }

    let pollens: Vec<String> = [
        (i18n::t("Aulne", "Alder"), c.alder_pollen),
        (i18n::t("Bouleau", "Birch"), c.birch_pollen),
        (i18n::t("Graminées", "Grass"), c.grass_pollen),
        (i18n::t("Armoise", "Mugwort"), c.mugwort_pollen),
        (i18n::t("Olivier", "Olive"), c.olive_pollen),
        (i18n::t("Ambroisie", "Ragweed"), c.ragweed_pollen),
    ]
    .into_iter()
    .filter_map(|(name, value)| value.filter(|&g| g >= 1.0).map(|g| format!("{name} {g:.0}")))
    .collect();
    let pollen_line = if pollens.is_empty() {
        i18n::t("Pollens : rien à signaler", "Pollen: nothing to report").to_owned()
    } else {
        tr!("Pollens (grains/m³) : {}", "Pollen (grains/m³): {}", pollens.join(" · "))
    };
    doc.text(&pollen_line, Style::default().small().center());
    Ok(doc)
}
