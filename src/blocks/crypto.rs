//! Cours de cryptomonnaies via l'API publique de CoinGecko (sans clé).

use std::collections::HashMap;

use anyhow::{bail, Context, Result};

use crate::doc::{Doc, Style};

/// « 74673.5 » -> « 74 673,50 ».
fn format_price(price: f64) -> String {
    let decimals = if price >= 1000.0 { 0 } else if price >= 1.0 { 2 } else { 4 };
    let s = format!("{price:.decimals$}");
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(c);
    }
    if frac.is_empty() { grouped } else { format!("{grouped},{frac}") }
}

pub fn build(agent: &ureq::Agent, coins: &[String], currency: &str) -> Result<Doc> {
    if coins.is_empty() {
        bail!("aucune cryptomonnaie demandée");
    }
    let currency = currency.to_lowercase();
    let prices: HashMap<String, HashMap<String, f64>> = agent
        .get("https://api.coingecko.com/api/v3/simple/price")
        .query("ids", coins.join(","))
        .query("vs_currencies", &currency)
        .query("include_24hr_change", "true")
        .call()
        .context("CoinGecko injoignable")?
        .body_mut()
        .read_json()?;
    let symbol = match currency.as_str() {
        "eur" => "€".to_owned(),
        "usd" => "$".to_owned(),
        other => other.to_uppercase(),
    };

    let mut doc = Doc::new();
    doc.header("Crypto");
    for coin in coins {
        let Some(p) = prices.get(coin) else {
            doc.line(&format!("{coin} : inconnu"), Style::default());
            continue;
        };
        let price = p.get(&currency).copied().unwrap_or(f64::NAN);
        let change = p.get(&format!("{currency}_24h_change")).copied().unwrap_or(0.0);
        let line = format!(
            "{:<12}{:>13} {:<2}{:>+7.1} %",
            crate::fr::capitalize(coin),
            format_price(price),
            symbol,
            change
        )
        .replace('.', ",");
        doc.line(&line, Style::default());
    }
    doc.text("Variation sur 24 h · CoinGecko", Style::default().small().center());
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices() {
        assert_eq!(format_price(74673.4), "74 673");
        assert_eq!(format_price(2296.82), "2 297");
        assert_eq!(format_price(0.5432), "0,5432");
        assert_eq!(format_price(12.5), "12,50");
    }
}
