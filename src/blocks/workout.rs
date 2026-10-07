//! Défi sportif du jour, sans équipement, sur trois niveaux. Local et hors ligne.

use anyhow::{bail, Result};
use chrono::NaiveDate;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde::Deserialize;

use crate::doc::{Align, Doc, Style};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Tout le monde peut le faire.
    #[serde(alias = "facile")]
    Easy,
    /// Là, on commence à causer.
    #[default]
    #[serde(alias = "moyen")]
    Medium,
    /// Réservé aux vrais déglingos.
    #[serde(alias = "difficile")]
    Hard,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Easy => "facile",
            Level::Medium => "moyen",
            Level::Hard => "difficile",
        }
    }

    fn tagline(self) -> &'static str {
        match self {
            Level::Easy => "Tout le monde peut le faire.",
            Level::Medium => "Là, on commence à causer.",
            Level::Hard => "Ça rigole plus : réservé aux vrais déglingos.",
        }
    }

    fn challenges(self) -> &'static [&'static str] {
        match self {
            Level::Easy => EASY,
            Level::Medium => MEDIUM,
            Level::Hard => HARD,
        }
    }
}

const EASY: &[&str] = &[
    "Prends les escaliers toute la journée : interdiction d'utiliser l'ascenseur ou l'escalator.",
    "3 séries de 10 montées sur la pointe des pieds, en te tenant à un mur.",
    "Marche 10 minutes juste après le déjeuner.",
    "Tiens en équilibre sur un pied pendant 30 secondes, chaque pied, à chaque brossage de dents.",
    "3 séries de 10 squats sur une chaise : assieds-toi et relève-toi sans les mains.",
    "Toutes les heures, lève-toi et étire-toi pendant une minute.",
    "5 minutes d'étirements ce soir : nuque, épaules, dos, cuisses.",
    "3 séries de 10 pompes contre un mur.",
    "Fais au moins 5 000 pas dans la journée.",
    "20 cercles de bras dans chaque sens, trois fois dans la journée.",
    "Marche pendant chacun de tes appels téléphoniques.",
    "Descends un arrêt plus tôt ou gare-toi plus loin : 10 minutes de marche en plus.",
    "3 fois 20 secondes de gainage sur les genoux.",
    "20 montées de genoux sur place, trois fois dans la journée.",
    "Relève-toi du sol 5 fois sans t'aider des mains.",
    "3 séries de 10 ponts fessiers, allongé sur le dos.",
    "Danse sur trois chansons d'affilée, même seul dans ta cuisine.",
    "2 séries de 8 fentes avant par jambe, en te tenant à une chaise.",
    "Lis ou travaille debout pendant 15 minutes de plus que d'habitude.",
    "3 séries de 10 battements de jambe sur le côté, chaque jambe, en te tenant à un mur.",
    "Monte et descends une marche pendant 2 minutes, sans t'arrêter.",
    "15 minutes de ménage en musique, à bon rythme.",
    "3 fois 30 secondes de chaise contre le mur (genoux pliés, dos au mur).",
    "Penche-toi doucement vers tes pieds jambes tendues, 10 fois, trois fois dans la journée.",
    "10 minutes de marche dehors, sans téléphone à la main.",
];

const MEDIUM: &[&str] = &[
    "50 pompes dans la journée, en autant de séries qu'il le faut (sur les genoux si besoin).",
    "100 squats dans la journée.",
    "3 fois 45 secondes de planche.",
    "3 séries de 15 fentes alternées.",
    "20 minutes de marche rapide d'une seule traite.",
    "3 fois 1 minute de chaise contre le mur.",
    "3 séries de 10 burpees.",
    "5 fois 30 secondes de jumping jacks, 30 secondes de repos entre chaque.",
    "3 séries de 12 dips sur une chaise.",
    "Fais 10 000 pas dans la journée.",
    "3 séries de 20 mountain climbers.",
    "3 fois 30 secondes de planche latérale de chaque côté.",
    "Monte 20 étages au total dans la journée.",
    "3 séries de 10 ponts fessiers sur une jambe, chaque jambe.",
    "4 fois 1 minute de petits sauts sur place, comme à la corde à sauter.",
    "3 séries de 20 crunchs et 3 séries de 15 relevés de jambes.",
    "Circuit en 3 tours : 10 pompes, 15 squats, 20 secondes de planche.",
    "3 séries de 10 squats sautés.",
    "3 fois 30 secondes de superman : à plat ventre, bras et jambes levés.",
    "15 minutes de course légère.",
    "3 séries de 8 pompes mains rapprochées.",
    "3 séries de 12 fentes arrière par jambe.",
    "3 fois 30 secondes de montées de genoux rapides, puis 30 secondes de talons-fesses.",
    "100 crunchs dans la journée.",
    "1 minute 30 de planche d'une traite.",
];

const HARD: &[&str] = &[
    "100 pompes dans la journée.",
    "100 burpees dans la journée.",
    "300 squats dans la journée.",
    "3 fois 2 minutes de planche.",
    "Circuit en 5 tours sans pause : 10 burpees, 20 squats sautés, 15 pompes, 30 secondes de planche.",
    "10 séries de 10 pompes : une série au début de chaque heure.",
    "5 minutes de chaise contre le mur au total, en deux fois maximum.",
    "3 séries de 10 pompes déclinées, les pieds sur une chaise.",
    "3 séries de 8 squats sur une jambe par jambe, en te tenant à un cadre de porte.",
    "3 séries de 10 pompes explosives : décolle les mains du sol à chaque montée.",
    "5 km de course.",
    "50 squats sautés et 50 fentes sautées.",
    "20 minutes : au début de chaque minute, 5 burpees et 5 squats sautés.",
    "3 fois 1 minute de planche latérale de chaque côté.",
    "3 séries de 15 dips sur une chaise, les pieds surélevés.",
    "Monte 30 étages dans la journée, dont 10 d'affilée en courant.",
    "200 mountain climbers.",
    "Tabata deux fois dans la journée : 8 fois 20 secondes de burpees et 10 secondes de repos.",
    "100 fentes sautées.",
    "3 fois 45 secondes de « hollow » : sur le dos, bras et jambes tendus et décollés du sol.",
    "3 séries de 10 pompes archer : le poids d'un côté, puis de l'autre.",
    "5 minutes de planche au total, en deux fois maximum.",
    "1 000 petits sauts sur place dans la journée.",
    "15 minutes d'allers-retours dans un escalier.",
    "Une pyramide de pompes : 1, 2, 3… jusqu'à 10, puis redescends jusqu'à 1.",
];

/// Défi d'un jour donné, pour un niveau : toute la liste passe avant la première répétition.
fn of_day(date: NaiveDate, level: Level) -> usize {
    let list = level.challenges();
    let mut order: Vec<usize> = (0..list.len()).collect();
    order.shuffle(&mut StdRng::seed_from_u64(7 + level as u64));
    let day = date.signed_duration_since(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()).num_days();
    order[day.rem_euclid(order.len() as i64) as usize]
}

pub fn build(today: NaiveDate, level: Level, number: Option<usize>) -> Result<Doc> {
    let list = level.challenges();
    let index = match number {
        Some(n) if (1..=list.len()).contains(&n) => n - 1,
        Some(n) => bail!("défi n° {n} inconnu (1 à {})", list.len()),
        None => of_day(today, level),
    };

    let mut doc = Doc::new();
    doc.header(&format!("Défi sportif · {}", level.label()));
    doc.text(level.tagline(), Style::default().small().center());
    doc.feed(1);
    doc.text(list[index], Style::default().bold());
    doc.text(&format!("n° {}", index + 1), Style::default().small().align(Align::Right));
    doc.feed(1);
    doc.text("[ ] Fait !", Style::default().size(2));
    doc.text("Écoute ton corps : adapte ou arrête en cas de douleur.", Style::default().small().center());
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_level_cycles_without_repeat() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        for level in [Level::Easy, Level::Medium, Level::Hard] {
            let n = level.challenges().len();
            let mut seen: Vec<usize> = (0..n as i64).map(|d| of_day(start + chrono::Duration::days(d), level)).collect();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), n);
        }
    }

    #[test]
    fn numbers_are_checked() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        assert!(build(day, Level::Hard, Some(1)).is_ok());
        assert!(build(day, Level::Hard, Some(99)).is_err());
    }
}
