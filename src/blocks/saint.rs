//! Saint du jour, d'après le calendrier traditionnel français (calendrier des postes).

use chrono::{Datelike, NaiveDate};

use crate::doc::{Doc, Style};

const JANVIER: [&str; 31] = [
    "Jour de l'An", "St Basile", "Ste Geneviève", "St Odilon", "St Édouard", "St Mélaine", "St Raymond",
    "St Lucien", "Ste Alix", "St Guillaume", "St Paulin", "Ste Tatiana", "Ste Yvette", "Ste Nina",
    "St Rémi", "St Marcel", "Ste Roseline", "Ste Prisca", "St Marius", "St Sébastien", "Ste Agnès",
    "St Vincent", "St Barnard", "St François de Sales", "Conversion de St Paul", "Ste Paule",
    "Ste Angèle", "St Thomas d'Aquin", "St Gildas", "Ste Martine", "Ste Marcelle",
];
const FEVRIER: [&str; 29] = [
    "Ste Ella", "Présentation du Seigneur", "St Blaise", "Ste Véronique", "Ste Agathe", "St Gaston",
    "Ste Eugénie", "Ste Jacqueline", "Ste Apolline", "St Arnaud", "Notre-Dame de Lourdes", "St Félix",
    "Ste Béatrice", "St Valentin", "St Claude", "Ste Julienne", "St Alexis", "Ste Bernadette",
    "St Gabin", "Ste Aimée", "St Damien", "Ste Isabelle", "St Lazare", "St Modeste", "St Roméo",
    "St Nestor", "Ste Honorine", "St Romain", "St Auguste",
];
const MARS: [&str; 31] = [
    "St Aubin", "St Charles le Bon", "St Guénolé", "St Casimir", "Ste Olive", "Ste Colette",
    "Ste Félicité", "St Jean de Dieu", "Ste Françoise", "St Vivien", "Ste Rosine", "Ste Justine",
    "St Rodrigue", "Ste Mathilde", "Ste Louise", "Ste Bénédicte", "St Patrice", "St Cyrille",
    "St Joseph", "St Herbert", "Ste Clémence", "Ste Léa", "St Victorien", "Ste Catherine de Suède",
    "Annonciation", "Ste Larissa", "St Habib", "St Gontran", "Ste Gwladys", "St Amédée",
    "St Benjamin",
];
const AVRIL: [&str; 30] = [
    "St Hugues", "Ste Sandrine", "St Richard", "St Isidore", "Ste Irène", "St Marcellin",
    "St Jean-Baptiste de la Salle", "Ste Julie", "St Gautier", "St Fulbert", "St Stanislas",
    "St Jules", "Ste Ida", "St Maxime", "St Paterne", "St Benoît-Joseph", "St Anicet", "St Parfait",
    "Ste Emma", "Ste Odette", "St Anselme", "St Alexandre", "St Georges", "St Fidèle", "St Marc",
    "Ste Alida", "Ste Zita", "Ste Valérie", "Ste Catherine de Sienne", "St Robert",
];
const MAI: [&str; 31] = [
    "Fête du Travail", "St Boris", "Sts Philippe et Jacques", "St Sylvain", "Ste Judith",
    "Ste Prudence", "Ste Gisèle", "Victoire 1945", "St Pacôme", "Ste Solange", "Ste Estelle",
    "St Achille", "Ste Rolande", "St Matthias", "Ste Denise", "St Honoré", "St Pascal", "St Éric",
    "St Yves", "St Bernardin", "St Constantin", "St Émile", "St Didier", "St Donatien", "Ste Sophie",
    "St Bérenger", "St Augustin de Cantorbéry", "St Germain", "St Aymar", "St Ferdinand",
    "Visitation de la Vierge Marie",
];
const JUIN: [&str; 30] = [
    "St Justin", "Ste Blandine", "St Kévin", "Ste Clotilde", "St Igor", "St Norbert", "St Gilbert",
    "St Médard", "Ste Diane", "St Landry", "St Barnabé", "St Guy", "St Antoine de Padoue",
    "St Élisée", "Ste Germaine", "St Jean-François Régis", "St Hervé", "St Léonce", "St Romuald",
    "St Silvère", "St Rodolphe", "St Alban", "Ste Audrey", "St Jean-Baptiste", "St Prosper",
    "St Anthelme", "St Fernand", "St Irénée", "Sts Pierre et Paul", "St Martial",
];
const JUILLET: [&str; 31] = [
    "St Thierry", "St Martinien", "St Thomas", "St Florent", "St Antoine", "Ste Mariette", "St Raoul",
    "St Thibaut", "Ste Amandine", "St Ulrich", "St Benoît", "St Olivier", "Sts Henri et Joël",
    "Fête nationale", "St Donald", "Notre-Dame du Mont-Carmel", "Ste Charlotte", "St Frédéric",
    "St Arsène", "Ste Marina", "St Victor", "Ste Marie-Madeleine", "Ste Brigitte", "Ste Christine",
    "St Jacques", "Sts Anne et Joachim", "Ste Nathalie", "St Samson", "Ste Marthe", "Ste Juliette",
    "St Ignace de Loyola",
];
const AOUT: [&str; 31] = [
    "St Alphonse", "St Julien Eymard", "Ste Lydie", "St Jean-Marie Vianney", "St Abel",
    "Transfiguration", "St Gaétan", "St Dominique", "St Amour", "St Laurent", "Ste Claire",
    "Ste Clarisse", "St Hippolyte", "St Évrard", "Assomption", "St Armel", "St Hyacinthe",
    "Ste Hélène", "St Jean Eudes", "St Bernard", "St Christophe", "St Fabrice", "Ste Rose de Lima",
    "St Barthélemy", "St Louis", "Ste Natacha", "Ste Monique", "St Augustin", "Ste Sabine",
    "St Fiacre", "St Aristide",
];
const SEPTEMBRE: [&str; 30] = [
    "St Gilles", "Ste Ingrid", "St Grégoire", "Ste Rosalie", "Ste Raïssa", "St Bertrand", "Ste Reine",
    "Nativité de la Vierge Marie", "St Alain", "Ste Inès", "St Adelphe", "St Apollinaire", "St Aimé",
    "Croix Glorieuse", "St Roland", "Ste Édith", "St Renaud", "Ste Nadège", "Ste Émilie", "St Davy",
    "St Matthieu", "St Maurice", "St Constant", "Ste Thècle", "St Hermann", "Sts Côme et Damien",
    "St Vincent de Paul", "St Venceslas", "Sts Michel, Gabriel et Raphaël", "St Jérôme",
];
const OCTOBRE: [&str; 31] = [
    "Ste Thérèse de l'Enfant-Jésus", "St Léger", "St Gérard", "St François d'Assise", "Ste Fleur",
    "St Bruno", "St Serge", "Ste Pélagie", "St Denis", "St Ghislain", "St Firmin", "St Wilfried",
    "St Géraud", "St Juste", "Ste Thérèse d'Avila", "Ste Edwige", "St Baudouin", "St Luc", "St René",
    "Ste Adeline", "Ste Céline", "Ste Élodie", "St Jean de Capistran", "St Florentin", "St Crépin",
    "St Dimitri", "Ste Émeline", "Sts Simon et Jude", "St Narcisse", "Ste Bienvenue", "St Quentin",
];
const NOVEMBRE: [&str; 30] = [
    "Toussaint", "Commémoration des défunts", "St Hubert", "St Charles", "Ste Sylvie", "Ste Bertille",
    "Ste Carine", "St Geoffroy", "St Théodore", "St Léon", "Armistice 1918", "St Christian",
    "St Brice", "St Sidoine", "St Albert", "Ste Marguerite", "Ste Élisabeth", "Ste Aude",
    "St Tanguy", "St Edmond", "Présentation de Marie", "Ste Cécile", "St Clément", "Ste Flora",
    "Ste Catherine", "Ste Delphine", "St Séverin", "St Jacques de la Marche", "St Saturnin",
    "St André",
];
const DECEMBRE: [&str; 31] = [
    "Ste Florence", "Ste Viviane", "St François-Xavier", "Ste Barbara", "St Gérald", "St Nicolas",
    "St Ambroise", "Immaculée Conception", "St Pierre Fourier", "St Romaric", "St Daniel",
    "Ste Jeanne-Françoise de Chantal", "Ste Lucie", "Ste Odile", "Ste Ninon", "Ste Alice", "St Gaël",
    "St Gatien", "St Urbain", "St Théophile", "St Pierre Canisius", "Ste Françoise-Xavière",
    "St Armand", "Ste Adèle", "Noël", "St Étienne", "St Jean", "Sts Innocents", "St David",
    "St Roger", "St Sylvestre",
];

pub fn of(date: NaiveDate) -> &'static str {
    let month: &[&str] = match date.month() {
        1 => &JANVIER,
        2 => &FEVRIER,
        3 => &MARS,
        4 => &AVRIL,
        5 => &MAI,
        6 => &JUIN,
        7 => &JUILLET,
        8 => &AOUT,
        9 => &SEPTEMBRE,
        10 => &OCTOBRE,
        11 => &NOVEMBRE,
        _ => &DECEMBRE,
    };
    month[date.day0() as usize]
}

pub fn build(date: NaiveDate) -> Doc {
    let mut doc = Doc::new();
    doc.text(&format!("Fête du jour : {}", of(date)), Style::default().center());
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_day_has_a_saint() {
        let mut d = NaiveDate::from_ymd_opt(2028, 1, 1).unwrap(); // année bissextile
        while d.year() == 2028 {
            assert!(!of(d).is_empty());
            d = d.succ_opt().unwrap();
        }
        assert_eq!(of(NaiveDate::from_ymd_opt(2026, 10, 7).unwrap()), "St Serge");
        assert_eq!(of(NaiveDate::from_ymd_opt(2026, 12, 25).unwrap()), "Noël");
    }
}
