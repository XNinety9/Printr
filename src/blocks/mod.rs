//! Blocs paramétrables d'un ticket, décrits en JSON.

mod agenda;
mod air_quality;
mod anagram;
mod arrow_word;
pub mod barnum;
mod bins;
mod birthdays;
mod chess;
mod chores;
mod cipher;
mod coloring;
mod coupon;
mod crossword;
mod crypto;
mod fortune;
mod glitch;
mod hashi;
mod holidays;
mod horoscope;
mod kakuro;
mod maze;
mod mental_math;
mod monthly;
mod moon;
mod news;
mod nonogram;
mod on_this_day;
mod petit_bac;
mod picto;
mod quote;
mod recipe;
mod riddle;
mod saint;
mod shopping;
mod sky;
mod story;
mod sudoku;
mod sun;
mod takuzu;
mod train_tracks;
mod weather;
mod weather_icon;
mod wifi;
mod word;
mod word_search;
mod workout;

use std::panic::AssertUnwindSafe;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::NaiveDate;
use rand::RngExt;
use serde::Deserialize;

use crate::cache::Cache;
use crate::claude::Claude;
use crate::doc::{Align, Doc, Op, Style};
use crate::{i18n, raster};

fn four() -> u8 {
    4
}

fn yes() -> bool {
    true
}
fn one() -> u8 {
    1
}
fn two() -> u8 {
    2
}
fn three() -> u8 {
    3
}
fn six() -> u8 {
    6
}
fn ten() -> u8 {
    10
}
fn default_coins() -> Vec<String> {
    vec!["bitcoin".to_owned(), "ethereum".to_owned()]
}
fn default_currency() -> String {
    "eur".to_owned()
}
fn default_max_age_hours() -> u32 {
    24
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    /// Coupe le papier à la fin.
    #[serde(default = "yes")]
    pub cut: bool,
    /// Lignes vides entre deux blocs.
    #[serde(default = "one")]
    pub spacing: u8,
    pub blocks: Vec<Block>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Title {
        text: String,
        #[serde(default = "two")]
        size: u8,
    },
    Text {
        text: String,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        underline: bool,
        #[serde(default)]
        reverse: bool,
        #[serde(default)]
        small: bool,
        #[serde(default = "one")]
        size: u8,
        #[serde(default)]
        align: Align,
    },
    /// Message étrange de la machine (ajouté aussi au hasard, voir `glitch::chance`).
    Glitch {
        #[serde(default)]
        seed: Option<u64>,
    },
    Separator {
        #[serde(default)]
        style: Option<char>,
    },
    /// La date du jour, ou une autre : `date` fixe (`AAAA-MM-JJ`), ou `offset` jours plus tard (ou
    /// plus tôt), pour préparer des tickets à l'avance.
    Date {
        #[serde(default)]
        date: Option<String>,
        #[serde(default)]
        offset: Option<i64>,
    },
    Feed {
        #[serde(default = "one")]
        lines: u8,
    },
    /// Coupe le papier : la suite du ticket sort sur un nouveau morceau.
    #[serde(alias = "coupe")]
    Cut {},
    Image {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        url: Option<String>,
        /// Photo envoyée depuis l'interface web (identifiant).
        #[serde(default)]
        upload: Option<String>,
        /// `false` : simple seuil noir/blanc, quel que soit `dither_algorithm`.
        #[serde(default = "yes")]
        dither: bool,
        #[serde(default)]
        dither_algorithm: raster::Dither,
    },
    Qr {
        data: String,
        #[serde(default)]
        size: Option<u8>,
        #[serde(default)]
        caption: Option<String>,
    },
    Weather {
        location: String,
        #[serde(default = "one")]
        days: u8,
        /// Comment s'habiller aujourd'hui, d'après les prévisions (sans IA).
        #[serde(default, alias = "tenue")]
        outfit: bool,
        /// Ligne du vent et de l'UV.
        #[serde(default = "yes")]
        wind: bool,
        /// Lever et coucher du soleil.
        #[serde(default = "yes")]
        sun: bool,
    },
    Horoscope {
        sign: String,
        #[serde(default)]
        tone: horoscope::Tone,
    },
    Saint {},
    Todo {
        #[serde(default)]
        title: Option<String>,
        items: Vec<String>,
    },
    Sudoku {
        #[serde(default)]
        difficulty: sudoku::Difficulty,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    /// Kakuro : des sommes à composer avec des chiffres de 1 à 9, à solution unique.
    Kakuro {
        #[serde(default)]
        difficulty: sudoku::Difficulty,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    /// Hashi : relier les îles par des ponts, à solution unique.
    #[serde(alias = "ponts")]
    Hashi {
        #[serde(default)]
        difficulty: sudoku::Difficulty,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    /// Takuzu (binairo) : des 0 et des 1, à solution unique.
    #[serde(alias = "binairo")]
    Takuzu {
        #[serde(default)]
        difficulty: sudoku::Difficulty,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    #[serde(alias = "mots_meles", alias = "mots_caches", alias = "mots_en_grille")]
    WordSearch {
        #[serde(default)]
        difficulty: sudoku::Difficulty,
        /// Thème des mots ; tiré du numéro de grille si absent.
        #[serde(default)]
        theme: Option<word_search::Theme>,
        /// Mots à cacher à la place d'un thème.
        #[serde(default)]
        words: Vec<String>,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    #[serde(alias = "voie_ferree", alias = "rails")]
    TrainTracks {
        #[serde(default)]
        difficulty: sudoku::Difficulty,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    Maze {
        #[serde(default)]
        width: Option<u8>,
        #[serde(default)]
        height: Option<u8>,
        #[serde(default)]
        seed: Option<u64>,
    },
    WordOfTheDay {},
    Holidays {
        #[serde(default)]
        zone: holidays::Zone,
        #[serde(default = "one")]
        count: u8,
    },
    Moon {},
    Countdown {
        label: String,
        date: NaiveDate,
    },
    OnThisDay {
        #[serde(default = "three")]
        count: u8,
    },
    AirQuality {
        location: String,
        /// Particules, ozone et dioxyde d'azote.
        #[serde(default = "yes")]
        pollutants: bool,
        #[serde(default = "yes")]
        pollen: bool,
    },
    Quote {},
    /// Un petit papier façon biscuit chinois (`fortune`), dit par une vache en option (`cowsay`).
    Fortune {
        #[serde(default)]
        source: fortune::Source,
        #[serde(default, alias = "vache")]
        cow: bool,
        #[serde(default)]
        seed: Option<u64>,
    },
    Crypto {
        #[serde(default = "default_coins")]
        coins: Vec<String>,
        #[serde(default = "default_currency")]
        currency: String,
    },
    Sun {
        location: String,
    },
    /// Horoscope hors ligne et gratuit, calculé sur le ciel réel par Barnum.
    Barnum {
        /// Signe (en français, accents facultatifs)…
        #[serde(default)]
        sign: Option<String>,
        /// … ou date de naissance, dont Barnum déduit le signe.
        #[serde(default)]
        birth_date: Option<NaiveDate>,
        /// Affiche la position des astres.
        #[serde(default)]
        sky: bool,
        /// Autre série de formules pour le même ciel (`--sel` de Barnum).
        #[serde(default)]
        variant: Option<String>,
    },
    /// Pictogramme dessiné : cœur, étoile, soleil, fleur, sourire.
    Picto {
        shape: picto::Shape,
        #[serde(default)]
        size: picto::Size,
        #[serde(default = "one")]
        count: u8,
    },
    /// Défi sportif du jour, sans équipement.
    #[serde(alias = "defi_sportif")]
    Workout {
        /// facile, moyen (défaut) ou difficile.
        #[serde(default)]
        level: workout::Level,
        #[serde(default)]
        number: Option<usize>,
    },
    /// Énigme du jour (devinette, charade, logique, calcul).
    #[serde(alias = "enigme")]
    Riddle {
        /// Limite à une famille : devinette, charade, logique, calcul.
        #[serde(default)]
        kind: Option<riddle::Kind>,
        /// Numéro d'une énigme précise (imprimé sous chaque énigme).
        #[serde(default)]
        number: Option<usize>,
        /// Réponse : envers (défaut), lendemain, dessous ou aucune.
        #[serde(default)]
        answer: riddle::Answer,
    },
    /// Petit bac : une lettre, des catégories, une feuille par joueur.
    PetitBac {
        #[serde(default)]
        letter: Option<char>,
        /// Catégories imposées ; tirées au hasard si vide.
        #[serde(default)]
        categories: Vec<String>,
        /// Nombre de catégories tirées au hasard.
        #[serde(default = "six")]
        count: u8,
        #[serde(default = "one")]
        players: u8,
        #[serde(default)]
        seed: Option<u64>,
    },
    /// Fiche de calcul mental, résultats à l'envers.
    #[serde(alias = "calcul_mental")]
    MentalMath {
        /// Niveau : `beginner` (débutant), `easy`, `medium`, `hard`.
        #[serde(default)]
        difficulty: mental_math::Level,
        /// Opérations à proposer (`addition`, `subtraction`, `multiplication`, `division`, ou
        /// `+ - × ÷`) ; toutes par défaut.
        #[serde(default)]
        operations: Vec<mental_math::Operation>,
        /// Tables à réviser pour les multiplications et divisions (de 1 à 12) ; celles du niveau
        /// par défaut.
        #[serde(default)]
        tables: Vec<u8>,
        #[serde(default = "ten")]
        count: u8,
        #[serde(default)]
        seed: Option<u64>,
    },
    /// Mots mystères : lettres mélangées, réponses à l'envers.
    #[serde(alias = "mot_mystere", alias = "anagramme")]
    Anagram {
        #[serde(default)]
        theme: Option<word_search::Theme>,
        #[serde(default = "three")]
        count: u8,
        #[serde(default)]
        seed: Option<u64>,
    },
    /// Logimage (picross) : un dessin à révéler.
    #[serde(alias = "logimage", alias = "picross")]
    Nonogram {
        #[serde(default)]
        number: Option<usize>,
        #[serde(default)]
        solution: bool,
    },
    /// Message codé (César, morse, nombres).
    #[serde(alias = "message_code")]
    Cipher {
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        cipher: cipher::Cipher,
        /// Décalage du code César (au hasard si absent).
        #[serde(default)]
        shift: Option<u8>,
        /// Message en clair, imprimé à l'envers en bas.
        #[serde(default = "yes")]
        answer: bool,
        #[serde(default)]
        seed: Option<u64>,
    },
    /// Liste de courses partagée de l'appli.
    #[serde(alias = "courses")]
    Shopping {
        #[serde(default)]
        title: Option<String>,
        /// Vide la liste une fois imprimée (impressions par le serveur).
        #[serde(default)]
        clear: bool,
    },
    /// Bon à offrir.
    #[serde(alias = "bon")]
    Coupon {
        /// « un petit-déjeuner au lit » ; une idée au hasard si absent.
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        to: Option<String>,
        #[serde(default)]
        valid_until: Option<NaiveDate>,
        #[serde(default = "one")]
        count: u8,
    },
    /// QR code de connexion au Wi-Fi.
    Wifi {
        ssid: String,
        #[serde(default)]
        password: Option<String>,
        #[serde(default)]
        security: wifi::Security,
        #[serde(default)]
        hidden: bool,
        #[serde(default = "yes")]
        show_password: bool,
    },
    /// Rappel des poubelles à sortir.
    #[serde(alias = "poubelles")]
    Bins {
        #[serde(default)]
        collections: Vec<bins::Collection>,
        #[serde(default)]
        when: bins::When,
        /// Imprime aussi les soirs sans ramassage (avec le prochain).
        #[serde(default)]
        always: bool,
    },
    /// Histoire du soir, inventée par Claude, avec les enfants pour héros.
    #[serde(alias = "histoire")]
    Story {
        /// Les prénoms : « Léo, Camille ».
        #[serde(default, alias = "enfants")]
        children: String,
        /// Un thème facultatif : « les dragons », « l'espace »…
        #[serde(default, alias = "theme")]
        theme: String,
        #[serde(default, alias = "longueur")]
        length: story::Length,
    },
    /// Anniversaires : celui du jour en grand, puis le compte à rebours des suivants.
    #[serde(alias = "anniversaires")]
    Birthdays {
        /// Une personne par ligne : « Léo, 12/03/2014 » (année facultative, pour l'âge).
        #[serde(default, alias = "liste")]
        list: String,
        /// Combien des suivants annoncer.
        #[serde(default = "three", alias = "suivants")]
        upcoming: u8,
        /// Ajoute les anniversaires communs à la famille (enregistrés dans l'appli).
        #[serde(default = "yes")]
        family: bool,
    },
    /// Problème d'échecs du jour (Lichess) : l'échiquier, et la solution à l'envers.
    #[serde(alias = "echecs")]
    Chess {
        #[serde(default = "yes")]
        solution: bool,
    },
    /// Tâches ménagères tournantes : chaque jour ou chaque semaine, elles passent à la personne suivante.
    #[serde(alias = "taches")]
    Chores {
        #[serde(default, alias = "taches")]
        tasks: Vec<String>,
        #[serde(default, alias = "qui")]
        people: Vec<String>,
        #[serde(default, alias = "rotation")]
        period: chores::Period,
    },
    /// Agenda du jour depuis des calendriers ICS.
    Agenda {
        #[serde(default)]
        calendars: Vec<String>,
        #[serde(default = "one")]
        days: u8,
        #[serde(default)]
        title: Option<String>,
    },
    /// Bilan du mois, façon ticket de caisse.
    #[serde(alias = "bilan")]
    MonthlyReport {
        /// « AAAA-MM » ; le mois en cours si absent.
        #[serde(default)]
        month: Option<String>,
    },
    /// Mots fléchés (CrossWords), définitions rédigées par Claude à l'impression.
    #[serde(alias = "mots_fleches", alias = "fleches")]
    ArrowWord {
        #[serde(default)]
        size: arrow_word::Size,
        /// 1 (facile) à 4 (très difficile) ; Claude Opus rédige les niveaux 3 et 4.
        #[serde(default = "two")]
        level: u8,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    /// Mots croisés (CrossWords), définitions rédigées par Claude à l'impression.
    #[serde(alias = "mots_croises", alias = "croises")]
    Crossword {
        #[serde(default)]
        size: crossword::Size,
        /// 1 (facile) à 4 (très difficile).
        #[serde(default = "two")]
        level: u8,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        solution: bool,
    },
    /// Recette lue sur une page web (données schema.org) : ingrédients à cocher, étapes.
    #[serde(alias = "recette")]
    Recipe {
        #[serde(default)]
        url: String,
        /// Imprimer les étapes (sinon, seulement les ingrédients : liste de courses).
        #[serde(default = "yes")]
        steps: bool,
        /// QR code vers la page, en bas du ticket.
        #[serde(default = "yes")]
        qr: bool,
    },
    /// Recette d'un plat, rédigée par Claude (Haiku par défaut) à l'impression.
    #[serde(alias = "recette_claude")]
    RecipeClaude {
        #[serde(default, alias = "plat")]
        dish: String,
        #[serde(default = "four")]
        servings: u8,
        #[serde(default = "yes")]
        steps: bool,
    },
    /// Qu'est-ce qu'on mange ? Les ingrédients qu'on a, Claude propose une recette qui s'en sert.
    #[serde(alias = "recette_ingredients", alias = "quest_ce_quon_mange")]
    RecipeFromIngredients {
        /// Un par ligne, ou séparés par des virgules.
        #[serde(default)]
        ingredients: String,
        #[serde(default = "four")]
        servings: u8,
        /// Une envie facultative : « rapide », « végétarien », « pour un pique-nique »…
        #[serde(default, alias = "envie")]
        wish: String,
        #[serde(default = "yes")]
        steps: bool,
    },
    /// Le ciel ce soir : planètes visibles, passages de l'ISS, lune, étoiles filantes.
    #[serde(alias = "ciel")]
    Sky {
        location: String,
    },
    /// Mandala à colorier.
    #[serde(alias = "coloriage")]
    Coloring {
        #[serde(default)]
        seed: Option<u64>,
    },
    /// Revue de presse à partir de flux RSS, sans IA.
    News {
        /// Titre du bandeau : « Actualités · <title> ».
        title: String,
        feeds: Vec<String>,
        #[serde(default = "three")]
        count: u8,
        /// Nombre de QR codes vers les articles les mieux classés (0 à 2).
        #[serde(default = "two")]
        qr: u8,
        /// Thèmes à privilégier (mots ou expressions).
        #[serde(default)]
        themes: Vec<String>,
        /// Mots qui écartent un article.
        #[serde(default)]
        exclude: Vec<String>,
        #[serde(default = "default_max_age_hours")]
        max_age_hours: u32,
    },
}

/// Ressources partagées par les blocs.
pub struct Ctx {
    pub today: NaiveDate,
    pub http: ureq::Agent,
    pub claude: Option<Claude>,
    pub cache: Option<Cache>,
    /// Ignore le contenu du cache (mais le met à jour).
    pub refresh: bool,
    /// Aperçu : aucun appel payant (Claude) ; les blocs concernés affichent un texte d'attente.
    pub preview: bool,
    /// Ligne de commande : fichiers locaux et réseau local permis. Depuis l'appli web ou un
    /// script (`Ctx::web`), seules les adresses publiques sont joignables, et aucun fichier
    /// local ne se lit.
    pub local: bool,
}

impl Ctx {
    /// Ticket lancé en ligne de commande, par quelqu'un qui a déjà la main sur la machine.
    pub fn new(refresh: bool) -> Self {
        Self::with_access(refresh, true)
    }

    /// Ticket venu de l'appli web, d'un script ou d'une planification.
    pub fn web(refresh: bool) -> Self {
        Self::with_access(refresh, false)
    }

    fn with_access(refresh: bool, local: bool) -> Self {
        Self {
            today: chrono::Local::now().date_naive(),
            http: crate::net::agent(!local),
            claude: Claude::from_env(),
            cache: Cache::open(),
            refresh,
            preview: false,
            local,
        }
    }

    /// Refuse la lecture d'un fichier local hors ligne de commande.
    pub fn check_local_file(&self, path: &str) -> Result<()> {
        anyhow::ensure!(
            self.local,
            "{}",
            tr!(
                "fichier local refusé depuis l'appli ({path}) : utilise une adresse web ou une photo envoyée",
                "local file refused from the app ({path}): use a web address or an uploaded photo"
            )
        );
        Ok(())
    }

    pub fn for_preview(mut self) -> Self {
        self.preview = true;
        self
    }
}

/// Texte d'attente d'un bloc généré par Claude, en aperçu (le vrai texte coûte un appel).
pub fn ai_placeholder(title: &str, what: &str) -> Doc {
    let mut doc = Doc::new();
    doc.header(title);
    doc.feed(1);
    doc.text(
        &tr!(
            "{what} sera rédigé par Claude au moment de l'impression.",
            "{what} will be written by Claude when printing."
        ),
        Style::default().center(),
    );
    doc
}

/// Bilan de la construction d'un bloc, pour la console et les réponses HTTP.
pub struct Report {
    pub label: String,
    pub elapsed: Duration,
    pub cached: bool,
    pub error: Option<String>,
}

impl Block {
    pub fn name(&self) -> &'static str {
        match self {
            Block::PetitBac { .. } => i18n::t("petit bac", "categories"),
            Block::MentalMath { .. } => i18n::t("calcul mental", "mental math"),
            Block::Anagram { .. } => i18n::t("mot mystère", "mystery word"),
            Block::Nonogram { .. } => i18n::t("logimage", "nonogram"),
            Block::Cipher { .. } => i18n::t("message codé", "secret message"),
            Block::Shopping { .. } => i18n::t("liste de courses", "shopping list"),
            Block::Coupon { .. } => i18n::t("bon à offrir", "gift coupon"),
            Block::Wifi { .. } => "Wi-Fi",
            Block::Bins { .. } => i18n::t("poubelles", "trash day"),
            Block::Chores { .. } => i18n::t("tâches ménagères", "chores"),
            Block::Chess { .. } => i18n::t("échecs", "chess"),
            Block::Birthdays { .. } => i18n::t("anniversaires", "birthdays"),
            Block::Story { .. } => i18n::t("histoire du soir", "bedtime story"),
            Block::Agenda { .. } => i18n::t("agenda", "calendar"),
            Block::MonthlyReport { .. } => i18n::t("bilan du mois", "monthly report"),
            Block::Coloring { .. } => i18n::t("coloriage", "coloring page"),
            Block::ArrowWord { .. } => i18n::t("mots fléchés", "arrow words"),
            Block::Crossword { .. } => i18n::t("mots croisés", "crossword"),
            Block::Recipe { .. } => i18n::t("recette", "recipe"),
            Block::Sky { .. } => i18n::t("ciel ce soir", "tonight's sky"),
            Block::RecipeClaude { .. } => i18n::t("recette par Claude", "Claude recipe"),
            Block::RecipeFromIngredients { .. } => i18n::t("qu'est-ce qu'on mange ?", "what's for dinner?"),
            Block::Title { .. } => i18n::t("titre", "title"),
            Block::Text { .. } => i18n::t("texte", "text"),
            Block::Glitch { .. } => "glitch",
            Block::Separator { .. } => i18n::t("séparateur", "separator"),
            Block::Date { .. } => "date",
            Block::Feed { .. } => i18n::t("espace", "space"),
            Block::Cut {} => i18n::t("coupe", "cut"),
            Block::Image { .. } => "image",
            Block::Qr { .. } => "QR code",
            Block::Weather { .. } => i18n::t("météo", "weather"),
            Block::Horoscope { .. } => "horoscope",
            Block::Saint {} => i18n::t("saint du jour", "name day"),
            Block::Todo { .. } => i18n::t("à faire", "to-do"),
            Block::Sudoku { .. } => "sudoku",
            Block::Takuzu { .. } => "takuzu",
            Block::Kakuro { .. } => "kakuro",
            Block::Hashi { .. } => "hashi",
            Block::WordSearch { .. } => i18n::t("mots mêlés", "word search"),
            Block::TrainTracks { .. } => i18n::t("voie ferrée", "train tracks"),
            Block::Maze { .. } => i18n::t("labyrinthe", "maze"),
            Block::WordOfTheDay {} => i18n::t("mot du jour", "word of the day"),
            Block::Holidays { .. } => i18n::t("jours fériés", "public holidays"),
            Block::Moon {} => i18n::t("lune", "moon"),
            Block::Countdown { .. } => i18n::t("compte à rebours", "countdown"),
            Block::OnThisDay { .. } => i18n::t("éphéméride", "on this day"),
            Block::AirQuality { .. } => i18n::t("qualité de l'air", "air quality"),
            Block::Quote {} => i18n::t("citation", "quote"),
            Block::Fortune { .. } => "fortune",
            Block::Crypto { .. } => "crypto",
            Block::Sun { .. } => i18n::t("lever/coucher du soleil", "sunrise/sunset"),
            Block::Riddle { .. } => i18n::t("énigme", "riddle"),
            Block::Workout { .. } => i18n::t("défi sportif", "workout"),
            Block::Picto { .. } => i18n::t("pictogramme", "pictogram"),
            Block::Barnum { .. } => i18n::t("horoscope Barnum", "Barnum horoscope"),
            Block::News { .. } => i18n::t("actualités", "news"),
        }
    }

    /// Nom du bloc, précisé par son paramètre principal : « météo · Lyon ».
    pub fn label(&self) -> String {
        let detail = match self {
            Block::Title { text, .. } => Some(text.chars().take(24).collect()),
            Block::Weather { location, .. } | Block::AirQuality { location, .. } => Some(location.clone()),
            Block::Horoscope { sign, tone } => Some(format!("{sign} ({})", tone.label())),
            Block::Sudoku { difficulty, solution, .. }
            | Block::TrainTracks { difficulty, solution, .. }
            | Block::Takuzu { difficulty, solution, .. }
            | Block::Kakuro { difficulty, solution, .. }
            | Block::Hashi { difficulty, solution, .. } => {
                Some(format!("{}{}", difficulty.label(), if *solution { ", solution" } else { "" }))
            }
            Block::WordSearch { difficulty, theme, words, solution, .. } => {
                let subject = match (words.is_empty(), theme) {
                    (false, _) => i18n::t("mes mots, ", "my words, "),
                    (true, Some(t)) => &format!("{}, ", t.label()),
                    (true, None) => "",
                };
                Some(format!("{subject}{}{}", difficulty.label(), if *solution { ", solution" } else { "" }))
            }
            Block::MentalMath { difficulty, operations, tables, .. } => {
                let mut detail = difficulty.label().to_owned();
                if !operations.is_empty() {
                    let ops: Vec<&str> = operations.iter().map(|o| o.symbol()).collect();
                    detail = format!("{detail}, {}", ops.join(" "));
                }
                if !tables.is_empty() {
                    let list: Vec<String> = tables.iter().map(u8::to_string).collect();
                    detail = format!("{detail}, {} {}", i18n::t("tables de", "tables of"), list.join(", "));
                }
                Some(detail)
            }
            Block::Cipher { cipher, .. } => Some(cipher.label().to_owned()),
            Block::Coupon { text, .. } => text.clone(),
            Block::Wifi { ssid, .. } => Some(ssid.clone()),
            Block::Agenda { title, .. } => title.clone(),
            Block::MonthlyReport { month, .. } => month.clone(),
            Block::Nonogram { number, .. } => number.map(|n| tr!("n° {n}", "No. {n}")),
            Block::ArrowWord { size, level, solution, .. } => Some(format!(
                "{}, {} {}{}",
                arrow_word::level_label(*level),
                i18n::t("grille", "grid"),
                size.label(),
                if *solution { ", solution" } else { "" }
            )),
            Block::Crossword { size, level, solution, .. } => Some(format!(
                "{}, {} {}{}",
                arrow_word::level_label(*level),
                i18n::t("grille", "grid"),
                size.label(),
                if *solution { ", solution" } else { "" }
            )),
            Block::Recipe { url, .. } => Some(url.split("://").nth(1).unwrap_or(url).chars().take(32).collect()),
            Block::RecipeClaude { dish, servings, .. } => Some(format!("{dish}, {servings}")),
            Block::Chores { people, period, .. } => Some(format!("{}, {}", people.join(", "), period.label())),
            Block::Story { children, .. } => Some(children.clone()),
            Block::RecipeFromIngredients { ingredients, .. } => {
                Some(ingredients.replace('\n', ", ").chars().take(32).collect())
            }
            Block::Countdown { label, .. } => Some(label.clone()),
            Block::Crypto { coins, .. } => Some(coins.join(", ")),
            Block::Sun { location } | Block::Sky { location } => Some(location.clone()),
            Block::News { title, .. } => Some(title.clone()),
            Block::Workout { level, .. } => Some(level.label().to_owned()),
            Block::Picto { shape, .. } => Some(shape.label().to_owned()),
            Block::Barnum { sign, birth_date, .. } => {
                birth_date.map(|d| tr!("né le {d}", "born {d}")).or_else(|| sign.clone())
            }
            Block::Image { path, url, upload, .. } => {
                path.as_ref().or(url.as_ref()).or(upload.as_ref()).map(|p| p.rsplit('/').next().unwrap_or(p).to_owned())
            }
            _ => None,
        };
        match detail {
            Some(d) => format!("{} · {d}", self.name()),
            None => self.name().to_owned(),
        }
    }

    pub fn build(&self, ctx: &Ctx) -> Result<Doc> {
        let mut doc = Doc::new();
        match self {
            Block::Title { text, size } => {
                doc.text(text, Style::default().bold().center().size(*size));
            }
            Block::Text { text, bold, underline, reverse, small, size, align } => {
                let style = Style {
                    bold: *bold,
                    underline: *underline,
                    reverse: *reverse,
                    small: *small,
                    align: *align,
                    ..Style::default()
                };
                doc.text(text, style.size(*size));
            }
            Block::Glitch { seed } => doc = glitch::build(*seed),
            Block::Separator { style } => {
                doc.rule(style.unwrap_or('-'));
            }
            Block::Date { date, offset } => {
                let day = match (date.as_deref().map(str::trim).filter(|d| !d.is_empty()), offset) {
                    (Some(d), _) => NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| {
                        anyhow::anyhow!(
                            "{}",
                            tr!("date invalide : {d} (AAAA-MM-JJ attendu)", "invalid date: {d} (YYYY-MM-DD expected)")
                        )
                    })?,
                    (None, Some(n)) => ctx.today + chrono::Duration::days((*n).clamp(-3660, 3660)),
                    (None, None) => ctx.today,
                };
                doc.text(&i18n::capitalize(&i18n::long_date(day)), Style::default().bold().center());
            }
            Block::Feed { lines } => {
                doc.feed(*lines);
            }
            Block::Cut {} => {
                doc.cut();
            }
            Block::Image { path, url, upload, dither, dither_algorithm } => {
                let dither = if *dither { *dither_algorithm } else { raster::Dither::None };
                let img = match (path, url, upload) {
                    (None, None, Some(id)) => {
                        anyhow::ensure!(
                            id.chars().all(|c| c.is_ascii_hexdigit()),
                            i18n::t("photo invalide", "invalid photo")
                        );
                        let dir = crate::store::uploads_dir()
                            .context(i18n::t("répertoire de données introuvable", "data directory not found"))?;
                        raster::load(&dir.join(format!("{id}.png")), dither)?
                    }
                    (Some(path), None, None) => {
                        ctx.check_local_file(path)?;
                        raster::load(path.as_ref(), dither)?
                    }
                    (None, Some(url), None) => {
                        let bytes = ctx
                            .http
                            .get(url)
                            .call()
                            .context(i18n::t("image injoignable", "image unreachable"))?
                            .body_mut()
                            .with_config()
                            .limit(20 * 1024 * 1024)
                            .read_to_vec()?;
                        raster::prepare(&raster::decode(&bytes)?, dither)
                    }
                    _ => anyhow::bail!(i18n::t(
                        "indiquer `path`, `url` ou `upload` (un seul)",
                        "give `path`, `url` or `upload` (only one)"
                    )),
                };
                doc.image(img);
            }
            Block::Qr { data, size, caption } => {
                doc.qr(data, size.unwrap_or(6));
                if let Some(caption) = caption {
                    doc.text(caption, Style::default().small().center());
                }
            }
            Block::Weather { location, days, outfit, wind, sun } => {
                doc = weather::build(
                    &ctx.http,
                    location,
                    *days,
                    weather::Show { outfit: *outfit, wind: *wind, sun: *sun },
                )?
            }
            Block::Horoscope { sign, tone } => doc = horoscope::build(ctx, sign, *tone)?,
            Block::Saint {} => doc = saint::build(ctx.today),
            Block::Todo { title, items } => {
                doc.header(title.as_deref().unwrap_or(i18n::t("À faire", "To do")));
                for item in items {
                    doc.hanging("[ ] ", item, Style::default());
                }
            }
            Block::Sudoku { difficulty, seed, solution } => doc = sudoku::build(*difficulty, *seed, *solution),
            Block::Takuzu { difficulty, seed, solution } => doc = takuzu::build(*difficulty, *seed, *solution),
            Block::Kakuro { difficulty, seed, solution } => doc = kakuro::build(*difficulty, *seed, *solution),
            Block::Hashi { difficulty, seed, solution } => doc = hashi::build(*difficulty, *seed, *solution),
            Block::WordSearch { difficulty, theme, words, seed, solution } => {
                doc = word_search::build(*difficulty, *theme, words, *seed, *solution);
            }
            Block::TrainTracks { difficulty, seed, solution } => {
                doc = train_tracks::build(*difficulty, *seed, *solution);
            }
            Block::Maze { width, height, seed } => {
                doc = maze::build(width.unwrap_or(12), height.unwrap_or(16), *seed);
            }
            Block::WordOfTheDay {} => doc = word::build(ctx)?,
            Block::Holidays { zone, count } => doc = holidays::build(ctx.today, *zone, *count),
            Block::Moon {} => doc = moon::build(chrono::Utc::now()),
            Block::Countdown { label, date } => {
                let days = (*date - ctx.today).num_days();
                let (big, small) = match days {
                    0 => (i18n::t("C'est aujourd'hui !", "It's today!").to_owned(), label.clone()),
                    d if d > 0 => (tr!("J-{d}", "D-{d}"), tr!("avant : {label}", "until: {label}")),
                    d => (tr!("J+{}", "D+{}", -d), tr!("depuis : {label}", "since: {label}")),
                };
                let size = if big.chars().count() <= 14 { 3 } else { 1 };
                doc.text(&big, Style::default().bold().center().size(size));
                doc.text(&small, Style::default().center());
            }
            Block::OnThisDay { count } => doc = on_this_day::build(&ctx.http, ctx.today, *count)?,
            Block::AirQuality { location, pollutants, pollen } => {
                doc = air_quality::build(&ctx.http, location, *pollutants, *pollen)?
            }
            Block::Quote {} => doc = quote::build(ctx.today),
            Block::Fortune { source, cow, seed } => doc = fortune::build(*source, *cow, *seed)?,
            Block::Crypto { coins, currency } => doc = crypto::build(&ctx.http, coins, currency)?,
            Block::Sun { location } => doc = sun::build(ctx, location)?,
            Block::Riddle { kind, number, answer } => doc = riddle::build(ctx.today, *kind, *number, *answer)?,
            Block::Workout { level, number } => doc = workout::build(ctx.today, *level, *number)?,
            Block::Picto { shape, size, count } => doc = picto::build(*shape, *size, *count),
            Block::Barnum { sign, birth_date, sky, variant } => {
                let who = match (birth_date, sign) {
                    (Some(birth), _) => barnum::Who::Birth(*birth),
                    (None, Some(sign)) => barnum::Who::Sign(sign.clone()),
                    (None, None) => {
                        anyhow::bail!(i18n::t("indiquer `sign` ou `birth_date`", "give `sign` or `birth_date`"))
                    }
                };
                doc = barnum::build(who, ctx.today, *sky, variant.as_deref())?;
            }
            Block::PetitBac { letter, categories, count, players, seed } => {
                doc = petit_bac::build(*letter, categories, *count, *players, *seed);
            }
            Block::MentalMath { difficulty, operations, tables, count, seed } => {
                let choice = mental_math::Choice { operations: operations.clone(), tables: tables.clone() };
                doc = mental_math::build(*difficulty, &choice, *count, *seed)
            }
            Block::Anagram { theme, count, seed } => doc = anagram::build(*theme, *count, *seed),
            Block::Nonogram { number, solution } => doc = nonogram::build(*number, *solution)?,
            Block::Cipher { message, cipher: kind, shift, answer, seed } => {
                doc = cipher::build(message.as_deref(), *kind, *shift, *answer, *seed);
            }
            Block::Shopping { title, .. } => doc = shopping::build(title.as_deref())?,
            Block::Coupon { text, from, to, valid_until, count } => {
                let coupon = coupon::Coupon {
                    text: text.as_deref(),
                    from: from.as_deref(),
                    to: to.as_deref(),
                    valid_until: *valid_until,
                    count: *count,
                };
                doc = coupon::build(&coupon, ctx.today);
            }
            Block::Wifi { ssid, password, security, hidden, show_password } => {
                doc = wifi::build(ssid, password.as_deref(), *security, *hidden, *show_password)?;
            }
            Block::Bins { collections, when, always } => doc = bins::build(collections, *when, *always, ctx.today)?,
            Block::Chores { tasks, people, period } => doc = chores::build(tasks, people, *period, ctx.today)?,
            Block::Chess { solution } => doc = chess::build(ctx, *solution)?,
            Block::Birthdays { list, upcoming, family } => {
                // Sans données (commande lancée hors du serveur), la liste de la famille est vide.
                let shared = if *family {
                    crate::store::read_data().map(|d| d.family_birthdays).unwrap_or_default()
                } else {
                    String::new()
                };
                doc = birthdays::build(list, &shared, *upcoming, ctx.today)?
            }
            Block::Story { children, theme, length } => doc = story::build(ctx, children, theme, *length)?,
            Block::Agenda { calendars, days, title } => doc = agenda::build(ctx, calendars, *days, title.as_deref())?,
            Block::MonthlyReport { month } => doc = monthly::build(month.as_deref(), ctx.today)?,
            Block::Coloring { seed } => doc = coloring::build(*seed),
            Block::ArrowWord { size, level, seed, solution } => {
                let options = arrow_word::Options { size: *size, level: *level, seed: *seed, solution: *solution };
                doc = arrow_word::build(ctx, &options)?;
            }
            Block::Recipe { url, steps, qr } => doc = recipe::build(ctx, url, *steps, *qr)?,
            Block::Sky { location } => doc = sky::build(ctx, location)?,
            Block::RecipeClaude { dish, servings, steps } => {
                doc = recipe::build_with_claude(ctx, dish, *servings, *steps)?
            }
            Block::RecipeFromIngredients { ingredients, servings, wish, steps } => {
                doc = recipe::build_from_ingredients(ctx, ingredients, *servings, wish, *steps)?
            }
            Block::Crossword { size, level, seed, solution } => {
                let options = crossword::Options { size: *size, level: *level, seed: *seed, solution: *solution };
                doc = crossword::build(ctx, &options)?;
            }
            Block::News { title, feeds, count, qr, themes, exclude, max_age_hours } => {
                let options = news::Options {
                    count: (*count).clamp(1, 5) as usize,
                    qr: *qr as usize,
                    themes,
                    exclude,
                    max_age: chrono::Duration::hours(*max_age_hours as i64),
                };
                doc = news::build(ctx, title, feeds, &options)?;
            }
        }
        Ok(doc)
    }
}

impl Ticket {
    /// Construit tous les blocs en parallèle (les requêtes réseau partent en même temps),
    /// puis les assemble dans l'ordre. Un bloc en échec est remplacé par un message.
    /// `progress` est prévenu au fil de l'eau, pour l'affichage en console.
    pub fn build(&self, ctx: &Ctx, progress: &mut dyn Progress) -> (Doc, Vec<Report>) {
        let labels: Vec<String> = self.blocks.iter().map(Block::label).collect();
        let mut done: Vec<Option<(Doc, Report)>> = self.blocks.iter().map(|_| None).collect();
        progress.start(&labels);

        std::thread::scope(|s| {
            let (tx, rx) = mpsc::channel();
            for (i, block) in self.blocks.iter().enumerate() {
                let tx = tx.clone();
                s.spawn(move || {
                    let start = Instant::now();
                    let result = std::panic::catch_unwind(AssertUnwindSafe(|| block.build(ctx)))
                        .unwrap_or_else(|_| Err(anyhow::anyhow!(i18n::t("erreur interne", "internal error"))))
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send((i, result, start.elapsed()));
                });
            }
            drop(tx);

            loop {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok((i, result, elapsed)) => {
                        let block = &self.blocks[i];
                        let (doc, error) = match result {
                            Ok(doc) => (doc, None),
                            Err(error) => {
                                let mut doc = Doc::new();
                                let message =
                                    tr!("({} indisponible : {error})", "({} unavailable: {error})", block.name());
                                doc.text(&message, Style::default().small().center());
                                (doc, Some(error))
                            }
                        };
                        let report = Report { label: labels[i].clone(), elapsed, cached: doc.from_cache, error };
                        progress.done(i, &report);
                        done[i] = Some((doc, report));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => progress.tick(),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        progress.finish();

        // De temps en temps, la machine glisse un message entre deux blocs, sans rien dire en console.
        let forced = self.blocks.iter().any(|b| matches!(b, Block::Glitch { .. }));
        let glitch_at = (!ctx.preview && !forced && glitch::chance()).then(|| rand::rng().random_range(0..=done.len()));

        let mut ticket = Doc::new();
        let mut reports = Vec::with_capacity(done.len());
        let mut parts: Vec<Doc> = Vec::with_capacity(done.len() + 1);
        for (doc, report) in done.into_iter().map(|d| d.expect("chaque bloc a répondu")) {
            parts.push(doc);
            reports.push(report);
        }
        if let Some(at) = glitch_at {
            parts.insert(at, glitch::build(None));
        }
        // Une coupe se place après l'espacement habituel, qui fait la marge sous le morceau
        // coupé ; le morceau suivant commence en haut, sans espacement.
        let mut after_cut = true;
        for part in parts {
            if !after_cut {
                ticket.feed(self.spacing);
            }
            after_cut = matches!(part.ops.as_slice(), [Op::Cut]);
            ticket.append(part);
        }
        (ticket, reports)
    }
}

/// Suivi de la construction d'un ticket (affichage en console).
pub trait Progress {
    fn start(&mut self, _labels: &[String]) {}
    /// Appelé régulièrement tant que des blocs sont en cours.
    fn tick(&mut self) {}
    fn done(&mut self, _index: usize, _report: &Report) {}
    fn finish(&mut self) {}
}

/// Aucun affichage (serveur, mode silencieux).
pub struct Silent;

impl Progress for Silent {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ticket_with_defaults() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [
                {"type": "title", "text": "Bonjour"},
                {"type": "saint"},
                {"type": "horoscope", "sign": "lion", "tone": "farfelu"},
                {"type": "sudoku", "difficulty": "facile"},
                {"type": "word_of_the_day"}
            ]}"#,
        )
        .unwrap();
        assert!(t.cut);
        assert_eq!(t.blocks.len(), 5);
    }

    #[test]
    fn parses_new_daily_blocks_and_defaults() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [
                {"type": "sun", "location": "Lyon"},
                {"type": "enigme", "kind": "charade", "answer": "lendemain"}
            ]}"#,
        )
        .unwrap();
        assert!(matches!(t.blocks[0], Block::Sun { ref location } if location == "Lyon"));
        assert!(matches!(t.blocks[1], Block::Riddle { kind: Some(riddle::Kind::Charade), .. }));
    }

    #[test]
    fn cut_block_comes_after_spacing_and_starts_next_piece_at_top() {
        let t: Ticket = serde_json::from_str(
            r#"{"spacing": 2, "blocks": [
                {"type": "text", "text": "avant"},
                {"type": "coupe"},
                {"type": "text", "text": "après"}
            ]}"#,
        )
        .unwrap();
        let (doc, _) = t.build(&Ctx::new(false).for_preview(), &mut Silent);
        let kinds: Vec<&str> = doc
            .ops
            .iter()
            .map(|op| match op {
                Op::Line { .. } => "ligne",
                Op::Cut => "coupe",
                Op::Feed(_) => "espace",
                _ => "autre",
            })
            .collect();
        assert_eq!(kinds, ["ligne", "espace", "coupe", "ligne"]);
        let preview = doc.preview(t.cut);
        assert_eq!(preview.matches('✂').count(), 2);
        assert_eq!(preview.matches('╭').count(), 2);
    }

    #[test]
    fn rejects_unknown_fields_and_types() {
        assert!(serde_json::from_str::<Ticket>(r#"{"blocks": [{"type": "saint", "oups": 1}]}"#).is_err());
        assert!(serde_json::from_str::<Ticket>(r#"{"blocks": [{"type": "licorne"}]}"#).is_err());
    }

    #[test]
    fn weather_and_air_show_everything_by_default() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [
                {"type": "weather", "location": "Lyon"},
                {"type": "weather", "location": "Lyon", "wind": false, "sun": false},
                {"type": "air_quality", "location": "Lyon"},
                {"type": "air_quality", "location": "Lyon", "pollutants": false}
            ]}"#,
        )
        .unwrap();
        assert!(matches!(t.blocks[0], Block::Weather { wind: true, sun: true, .. }));
        assert!(matches!(t.blocks[1], Block::Weather { wind: false, sun: false, .. }));
        assert!(matches!(t.blocks[2], Block::AirQuality { pollutants: true, pollen: true, .. }));
        assert!(matches!(t.blocks[3], Block::AirQuality { pollutants: false, pollen: true, .. }));
    }

    #[test]
    fn date_block_prints_another_day() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [
                {"type": "date", "offset": 2},
                {"type": "date", "date": "2026-12-25"},
                {"type": "date", "date": "25/12/2026"}
            ]}"#,
        )
        .unwrap();
        let ctx = Ctx {
            today: NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
            http: ureq::agent(),
            claude: None,
            cache: None,
            refresh: false,
            preview: false,
            local: true,
        };
        let (doc, reports) = t.build(&ctx, &mut Silent);
        let preview = doc.preview(false);
        // Deux jours après le 7 octobre, puis Noël ; la troisième date est refusée.
        assert!(preview.contains("9 octobre 2026") || preview.contains("October 9, 2026"), "{preview}");
        assert!(preview.contains("25 décembre 2026") || preview.contains("December 25, 2026"), "{preview}");
        assert_eq!(reports.iter().filter(|r| r.error.is_some()).count(), 1);
    }

    #[test]
    fn failing_block_does_not_break_ticket() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [{"type": "text", "text": "avant"}, {"type": "image", "path": "/nope.png"}, {"type": "text", "text": "après"}]}"#,
        )
        .unwrap();
        let ctx = Ctx {
            today: NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
            http: ureq::agent(),
            claude: None,
            cache: None,
            refresh: false,
            preview: false,
            local: true,
        };
        let (doc, reports) = t.build(&ctx, &mut Silent);
        let preview = doc.preview(true);
        assert_eq!(reports.iter().filter(|r| r.error.is_some()).count(), 1);
        assert!(preview.contains("avant") && preview.contains("après"));
        assert!(preview.contains("image indisponible"));
    }

    #[test]
    fn build_reports_each_block_and_keeps_ticket_order() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [
                {"type": "title", "text": "premier"},
                {"type": "image", "path": "/nope.png"},
                {"type": "title", "text": "dernier"}
            ]}"#,
        )
        .unwrap();
        let ctx = Ctx {
            today: NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
            http: ureq::agent(),
            claude: None,
            cache: None,
            refresh: false,
            preview: false,
            local: true,
        };
        /// Note l'ordre dans lequel les blocs se terminent.
        struct Recorder(Vec<String>);
        impl Progress for Recorder {
            fn done(&mut self, _index: usize, report: &Report) {
                self.0.push(report.label.clone());
            }
        }
        let mut completed = Recorder(Vec::new());
        let (doc, reports) = t.build(&ctx, &mut completed);

        assert_eq!(completed.0.len(), 3);
        assert_eq!(
            reports.iter().map(|report| report.label.as_str()).collect::<Vec<_>>(),
            ["titre · premier", "image · nope.png", "titre · dernier"]
        );
        let titles: Vec<_> = doc
            .ops
            .iter()
            .filter_map(|op| match op {
                crate::doc::Op::Line { text, .. } if text == "premier" || text == "dernier" => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(titles, ["premier", "dernier"]);
    }

    /// Un bloc tout juste ajouté dans l'appli arrive sans ses champs vides : le ticket doit se lire
    /// quand même, et seul ce bloc est en erreur (l'aperçu du reste s'affiche).
    #[test]
    fn freshly_added_empty_blocks_do_not_break_the_ticket() {
        let t: Ticket = serde_json::from_str(
            r#"{"blocks": [
                {"type": "story"}, {"type": "birthdays"}, {"type": "chores"},
                {"type": "recipe_from_ingredients"}, {"type": "recipe_claude"}, {"type": "recipe"},
                {"type": "agenda"}, {"type": "bins"},
                {"type": "text", "text": "toujours là"}
            ]}"#,
        )
        .expect("le ticket se lit");
        let (doc, reports) = t.build(&Ctx::new(false).for_preview(), &mut Silent);
        assert_eq!(reports.iter().filter(|r| r.error.is_some()).count(), 8);
        assert!(doc.preview(false).contains("toujours là"));
    }
}
