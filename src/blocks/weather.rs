//! Météo via Open-Meteo (gratuit, sans clé) : géocodage puis prévisions.

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde::Deserialize;

use super::weather_icon::{self, Sky};
use crate::doc::{Doc, Style};
use crate::{draw, i18n};

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
    #[serde(default)]
    wind_speed_10m_max: Vec<Option<f64>>,
}

/// Libellé d'un code météo WMO, en français ou en anglais.
fn describe(code: u8) -> &'static str {
    use crate::i18n::t;
    match code {
        0 => t("Ciel dégagé", "Clear sky"),
        1 => t("Plutôt dégagé", "Mainly clear"),
        2 => t("Partiellement nuageux", "Partly cloudy"),
        3 => t("Couvert", "Overcast"),
        45 => t("Brouillard", "Fog"),
        48 => t("Brouillard givrant", "Rime fog"),
        51 => t("Bruine légère", "Light drizzle"),
        53 => t("Bruine", "Drizzle"),
        55 => t("Bruine dense", "Dense drizzle"),
        56 | 57 => t("Bruine verglaçante", "Freezing drizzle"),
        61 => t("Pluie faible", "Light rain"),
        63 => t("Pluie", "Rain"),
        65 => t("Forte pluie", "Heavy rain"),
        66 | 67 => t("Pluie verglaçante", "Freezing rain"),
        71 => t("Neige faible", "Light snow"),
        73 => t("Neige", "Snow"),
        75 => t("Forte neige", "Heavy snow"),
        77 => t("Grains de neige", "Snow grains"),
        80 => t("Averses faibles", "Light showers"),
        81 => t("Averses", "Showers"),
        82 => t("Fortes averses", "Heavy showers"),
        85 => t("Averses de neige", "Snow showers"),
        86 => t("Fortes averses de neige", "Heavy snow showers"),
        95 => t("Orage", "Thunderstorm"),
        96 | 99 => t("Orage et grêle", "Thunderstorm with hail"),
        _ => t("Temps indéterminé", "Unknown weather"),
    }
}

/// « Lyon », ou des coordonnées « 45.76,4.83 ».
pub fn locate(agent: &ureq::Agent, location: &str) -> Result<Place> {
    if let Some((lat, lon)) = location.split_once(',')
        && let (Ok(latitude), Ok(longitude)) = (lat.trim().parse(), lon.trim().parse())
    {
        return Ok(Place { name: location.to_owned(), latitude, longitude });
    }
    let geo: GeoResults = agent
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query("name", location)
        .query("count", "1")
        .query("language", i18n::lang().code())
        .call()
        .context(i18n::t("géocodage injoignable", "geocoding unreachable"))?
        .body_mut()
        .read_json()?;
    match geo.results.and_then(|r| r.into_iter().next()) {
        Some(place) => Ok(place),
        None => bail!("{}", tr!("lieu introuvable : {location}", "place not found: {location}")),
    }
}

/// Heure « HH:MM » d'un horodatage ISO « 2026-10-07T07:52 ».
fn hour(iso: &str) -> &str {
    iso.split_once('T').map_or(iso, |(_, t)| t)
}

/// Ce que montre le bloc, en plus des températures et de la pluie.
#[derive(Clone, Copy, Debug)]
pub struct Show {
    pub outfit: bool,
    pub wind: bool,
    pub sun: bool,
}

pub fn build(agent: &ureq::Agent, location: &str, days: u8, show: Show) -> Result<Doc> {
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
             sunrise,sunset,uv_index_max,wind_speed_10m_max",
        )
        .query("timezone", "auto")
        .query("forecast_days", days.to_string())
        .call()
        .context(i18n::t("Open-Meteo injoignable", "Open-Meteo unreachable"))?
        .body_mut()
        .read_json()?;
    let (c, d) = (&f.current, &f.daily);
    let pct = |v: Option<f64>| v.map_or("-".to_owned(), |p| format!("{p:.0}%"));

    let mut doc = Doc::new();
    doc.header(&tr!("Météo · {}", "Weather · {}", place.name));
    doc.image(centered(weather_icon::render(Sky::from_code(c.weather_code), TODAY_ICON)));
    doc.text(describe(c.weather_code), Style::default().bold().center());
    doc.text(
        &tr!("{:.0}°C (ressenti {:.0}°C)", "{:.0}°C (feels {:.0}°C)", c.temperature_2m, c.apparent_temperature),
        Style::default().size(2).center(),
    );
    doc.feed(1);
    doc.text(
        &tr!(
            "Min {:.0}°C · Max {:.0}°C · Pluie {}",
            "Min {:.0}°C · Max {:.0}°C · Rain {}",
            d.temperature_2m_min[0],
            d.temperature_2m_max[0],
            pct(d.precipitation_probability_max[0])
        ),
        Style::default().center(),
    );
    if show.wind {
        let uv = d.uv_index_max[0].map_or("-".to_owned(), |u| format!("{u:.0}"));
        doc.text(
            &tr!("Vent {:.0} km/h · UV {uv}", "Wind {:.0} km/h · UV {uv}", c.wind_speed_10m),
            Style::default().center(),
        );
    }
    if show.sun {
        doc.text(&tr!("Soleil {} - {}", "Sun {} - {}", hour(&d.sunrise[0]), hour(&d.sunset[0])), Style::default().center());
    }
    if show.outfit {
        let today = Today {
            min: d.temperature_2m_min[0],
            max: d.temperature_2m_max[0],
            rain: d.precipitation_probability_max[0].unwrap_or(0.0),
            wind: d.wind_speed_10m_max.first().copied().flatten().unwrap_or(c.wind_speed_10m),
            uv: d.uv_index_max[0].unwrap_or(0.0),
            sky: Sky::from_code(d.weather_code[0]),
        };
        doc.feed(1);
        doc.text(i18n::t("Comment s'habiller", "What to wear"), Style::default().bold().underline());
        for advice in what_to_wear(&today) {
            doc.hanging("- ", &advice, Style::default());
        }
    }

    if d.time.len() > 1 {
        doc.feed(1);
        let days: Vec<Day> = (1..d.time.len())
            .map(|i| Day {
                label: format!("{} {}", i18n::weekday_short(d.time[i]).trim_end_matches('.'), d.time[i].format("%d")),
                sky: Sky::from_code(d.weather_code[i]),
                min: d.temperature_2m_min[i],
                max: d.temperature_2m_max[i],
                rain: d.precipitation_probability_max[i],
            })
            .collect();
        doc.image(forecast(&days));
    }
    Ok(doc)
}

/// Le temps d'aujourd'hui, pour les conseils d'habillement.
struct Today {
    min: f64,
    max: f64,
    /// Risque de pluie, en %.
    rain: f64,
    /// Vent le plus fort de la journée, en km/h.
    wind: f64,
    uv: f64,
    sky: Sky,
}

/// Comment s'habiller, d'après des seuils simples (pas d'IA) : couches selon le froid, parapluie ou
/// imperméable, bottes, protection du soleil.
fn what_to_wear(day: &Today) -> Vec<String> {
    let mut out = Vec::new();
    let layer = |t: f64| {
        if t < 0.0 {
            i18n::t("manteau chaud, bonnet, écharpe et gants", "warm coat, hat, scarf and gloves")
        } else if t < 8.0 {
            i18n::t("manteau et bonnet", "coat and hat")
        } else if t < 14.0 {
            i18n::t("une veste ou un bon pull", "a jacket or a warm sweater")
        } else if t < 19.0 {
            i18n::t("un pull léger ou un gilet", "a light sweater or a cardigan")
        } else if t < 25.0 {
            i18n::t("un t-shirt suffit", "a t-shirt is enough")
        } else {
            i18n::t("tenue légère, et de l'eau", "light clothes, and some water")
        }
    };
    let (morning, afternoon) = (layer(day.min), layer(day.max));
    if morning == afternoon {
        out.push(i18n::capitalize(morning));
    } else {
        out.push(tr!(
            "Le matin : {morning} ; l'après-midi : {afternoon}",
            "Morning: {morning}; afternoon: {afternoon}"
        ));
    }
    let wet = day.rain >= 50.0 || matches!(day.sky, Sky::Rain | Sky::Drizzle | Sky::Storm);
    if day.sky == Sky::Snow {
        out.push(i18n::t("Bottes : il neige", "Boots: it's snowing").to_owned());
    } else if wet && day.wind >= 40.0 {
        out.push(
            i18n::t(
                "Un imperméable plutôt qu'un parapluie : ça souffle",
                "A raincoat rather than an umbrella: it's windy",
            )
            .to_owned(),
        );
    } else if wet {
        out.push(i18n::t("Prends un parapluie", "Take an umbrella").to_owned());
    } else if day.rain >= 30.0 {
        out.push(i18n::t("Un parapluie dans le sac, au cas où", "An umbrella in the bag, just in case").to_owned());
    } else if day.wind >= 50.0 {
        out.push(i18n::t("Coupe-vent : ça souffle fort", "Windbreaker: it's very windy").to_owned());
    }
    if day.uv >= 6.0 {
        out.push(i18n::t("Casquette, lunettes et crème solaire", "Cap, sunglasses and sunscreen").to_owned());
    } else if day.uv >= 3.0 && day.sky == Sky::Clear {
        out.push(i18n::t("Lunettes de soleil", "Sunglasses").to_owned());
    }
    out
}

/// Côté du pictogramme du jour, en points.
const TODAY_ICON: u32 = 150;

/// Un pictogramme au milieu d'une bande pleine largeur.
fn centered(icon: image::GrayImage) -> image::GrayImage {
    let mut img = draw::canvas(icon.height());
    let x = (img.width() - icon.width()) / 2;
    image::imageops::overlay(&mut img, &icon, i64::from(x), 0);
    img
}

/// Un jour des prévisions.
struct Day {
    label: String,
    sky: Sky,
    min: f64,
    max: f64,
    rain: Option<f64>,
}

/// Les jours suivants en colonnes : jour, pictogramme, maximale (en gros), minimale, risque de pluie.
fn forecast(days: &[Day]) -> image::GrayImage {
    let width = i64::from(crate::raster::PRINT_WIDTH);
    let column = width / days.len().max(1) as i64;
    let icon = (column - 14).clamp(40, 72) as u32;
    let (small, big) = (2, 3);
    let line = draw::DIGIT_H * small;
    let (y_icon, y_max) = (line + 6, line + 6 + i64::from(icon) + 6);
    let y_min = y_max + draw::DIGIT_H * big + 6;
    let y_rain = y_min + line + 4;
    let mut img = draw::canvas((y_rain + line) as u32);
    for (k, day) in days.iter().enumerate() {
        let center = k as i64 * column + column / 2;
        let mut put = |text: &str, y: i64, scale: i64| {
            draw::text(&mut img, text, center - draw::text_width(text, scale) / 2, y, scale);
        };
        put(&day.label, 0, small);
        put(&format!("{:.0}°", day.max), y_max, big);
        put(&format!("{:.0}°", day.min), y_min, small);
        if let Some(rain) = day.rain.filter(|r| *r >= 10.0) {
            put(&format!("{rain:.0}%"), y_rain, small);
        }
        let pictogram = weather_icon::render(day.sky, icon);
        image::imageops::overlay(&mut img, &pictogram, center - i64::from(icon) / 2, y_icon);
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_to_wear_follows_the_forecast() {
        let day = |min, max, rain, wind, uv, sky| Today { min, max, rain, wind, uv, sky };
        let fr = |d: &Today| crate::i18n::with_lang(crate::i18n::Lang::Fr, || what_to_wear(d));
        assert_eq!(fr(&day(-3.0, -1.0, 0.0, 10.0, 1.0, Sky::Clear)), ["Manteau chaud, bonnet, écharpe et gants"]);
        let rainy = fr(&day(6.0, 13.0, 80.0, 20.0, 1.0, Sky::Rain));
        assert_eq!(rainy[0], "Le matin : manteau et bonnet ; l'après-midi : une veste ou un bon pull");
        assert_eq!(rainy[1], "Prends un parapluie");
        assert!(fr(&day(8.0, 12.0, 70.0, 55.0, 1.0, Sky::Rain))[1].starts_with("Un imperméable"));
        let summer = fr(&day(20.0, 31.0, 0.0, 10.0, 8.0, Sky::Clear));
        assert_eq!(summer.last().map(String::as_str), Some("Casquette, lunettes et crème solaire"));
    }

    #[test]
    fn weather_codes_follow_language() {
        assert_eq!(describe(3), "Couvert");
        crate::i18n::with_lang(crate::i18n::Lang::En, || assert_eq!(describe(3), "Overcast"));
    }
}
