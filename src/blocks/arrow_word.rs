//! Mots fléchés, générés par CrossWords (https://github.com/XNinety9/CrossWords).
//!
//! La bibliothèque remplit la grille ; on la relit dans son format JSON documenté (version 1,
//! `docs/format-json.md`), qui reste le contrat entre les deux projets. Rendu « à numéros »,
//! adapté au papier de 80 mm : la grille en image, cases définitions noires numérotées et
//! flèches de départ, puis les définitions numérotées en texte.
//!
//! Définitions : le cache de CrossWords (`definitions.txt`), complété à l'impression par Claude
//! pour les mots inconnus (jamais en aperçu). Grille et définitions sont gardées par numéro :
//! la solution se réimprime à l'identique, sans nouvel appel.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, anyhow, ensure};
use crosswords::{Definitions, Dictionary, FlechesOptions, JSON_FORMAT_VERSION, autodefs, generate_fleches, level_settings, wordlist};
use image::GrayImage;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::Ctx;
use crate::doc::{Align, Doc, Style};
use crate::draw;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    #[serde(alias = "petit", alias = "petite")]
    Small,
    #[default]
    #[serde(alias = "moyen", alias = "moyenne")]
    Medium,
    #[serde(alias = "grand", alias = "grande")]
    Large,
}

impl Size {
    /// Colonnes × lignes : 9 colonnes donnent des cases de 7 mm, de quoi écrire au stylo.
    fn dims(self) -> (usize, usize) {
        match self {
            Size::Small => (7, 9),
            Size::Medium => (9, 11),
            Size::Large => (11, 13),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Size::Small => "petite",
            Size::Medium => "moyenne",
            Size::Large => "grande",
        }
    }
}

pub fn level_label(level: u8) -> &'static str {
    match level {
        1 => "facile",
        2 => "moyen",
        3 => "difficile",
        _ => "très difficile",
    }
}

// ---- Format JSON de CrossWords, version 1 ----

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Grille {
    format: String,
    version: u32,
    #[serde(rename = "type")]
    kind: String,
    largeur: usize,
    hauteur: usize,
    graine: Option<u64>,
    niveau: Option<u8>,
    grille: Vec<String>,
    mots: Vec<Mot>,
    definitions: Vec<Def>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Mot {
    id: usize,
    mot: String,
    sens: String,
    ligne: usize,
    colonne: usize,
    longueur: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Def {
    numero: u32,
    ligne: usize,
    colonne: usize,
    fleche: String,
    mot: usize,
    texte: Option<String>,
}

/// Grille gardée en cache, avec ce qu'on sait de ses définitions.
#[derive(Serialize, Deserialize)]
struct Saved {
    grille: Grille,
    /// Claude a déjà été sollicité : les mots encore sans définition ne lui seront pas
    /// redemandés à chaque impression (ni la dépense avec).
    claude_tried: bool,
}

/// Lit une grille CrossWords ; refuse une version du format qu'on ne connaît pas.
fn parse(json: &str) -> Result<Grille> {
    let g: Grille = serde_json::from_str(json).context("grille CrossWords illisible")?;
    ensure!(g.format == "crosswords", "ce n'est pas une grille CrossWords");
    ensure!(g.version == JSON_FORMAT_VERSION, "format de grille CrossWords inconnu (version {})", g.version);
    ensure!(g.kind == "fleches", "grille de type « {} », des mots fléchés étaient attendus", g.kind);
    ensure!(
        g.grille.len() == g.hauteur && g.grille.iter().all(|row| row.len() == g.largeur),
        "dimensions de grille incohérentes"
    );
    ensure!(g.definitions.iter().all(|d| d.mot < g.mots.len()), "définition sans mot");
    Ok(g)
}

impl Grille {
    fn is_block(&self, row: usize, col: usize) -> bool {
        self.grille[row].as_bytes()[col] == b'#'
    }

    /// Mots encore sans définition, sans doublon.
    fn missing(&self) -> Vec<String> {
        let mut words: Vec<String> =
            self.definitions.iter().filter(|d| d.texte.is_none()).map(|d| self.mots[d.mot].mot.clone()).collect();
        words.sort();
        words.dedup();
        words
    }
}

// ---- Dictionnaire et définitions de CrossWords ----

/// Dictionnaire par seuil de fréquence, chargé une fois (la liste est téléchargée par curl au
/// tout premier usage, dans le cache de CrossWords).
fn dictionary(min_score: u32) -> Result<Arc<Dictionary>> {
    static DICTS: OnceLock<Mutex<HashMap<u32, Arc<Dictionary>>>> = OnceLock::new();
    let mut dicts = DICTS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(dict) = dicts.get(&min_score) {
        return Ok(Arc::clone(dict));
    }
    let path = wordlist::french(|_| {}).context("liste de mots de CrossWords indisponible (premier téléchargement)")?;
    let file = File::open(&path).with_context(|| format!("{} illisible", path.display()))?;
    let dict = Arc::new(Dictionary::from_reader(BufReader::new(file), min_score)?);
    dicts.insert(min_score, Arc::clone(&dict));
    Ok(dict)
}

fn definitions_path() -> Option<PathBuf> {
    wordlist::cache_dir().map(|dir| dir.join("definitions.txt"))
}

/// Toutes les définitions connues (tous niveaux) : le cache de CrossWords.
fn known_definitions() -> Definitions {
    definitions_path()
        .and_then(|p| File::open(p).ok())
        .and_then(|f| Definitions::from_reader(BufReader::new(f)).ok())
        .unwrap_or_default()
}

/// Ajoute des définitions rédigées au cache de CrossWords, au même format que sa commande.
fn remember(defs: &[(String, String)], level: u8, model: &str) -> Result<()> {
    let Some(path) = definitions_path() else { return Ok(()) };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = format!("# Rédigées par {model}, niveau {level} : à relire\n");
    for (word, def) in defs {
        text.push_str(&format!("{word}\t{def}\t{level}\n"));
    }
    OpenOptions::new().create(true).append(true).open(&path)?.write_all(text.as_bytes())?;
    Ok(())
}

fn generate(width: usize, height: usize, level: u8, seed: u64) -> Result<Grille> {
    let dict = dictionary(level_settings(level).min_score)?;
    let mut options = FlechesOptions::new(width, height).with_level(level);
    options.seed = seed;
    let (fleches, _) = generate_fleches(&dict, &options).map_err(|e| anyhow!("grille impossible : {e}"))?;
    let defs = known_definitions().at_level(level);
    parse(&fleches.to_json(Some(&defs), Some(seed), Some(level)))
}

/// Fait rédiger par Claude les définitions qui manquent. Renvoie le nombre de mots restés
/// sans définition.
fn complete(grille: &mut Grille, level: u8) -> Result<usize> {
    let missing = grille.missing();
    if missing.is_empty() {
        return Ok(0);
    }
    let report = autodefs::generate(&missing, &known_definitions(), level, |_| {})
        .map_err(|e| anyhow!("définitions par Claude impossibles : {e}"))?;
    remember(&report.definitions, level, &report.model)?;
    let written: HashMap<&str, &str> = report.definitions.iter().map(|(w, d)| (w.as_str(), d.as_str())).collect();
    for def in grille.definitions.iter_mut().filter(|d| d.texte.is_none()) {
        if let Some(text) = written.get(grille.mots[def.mot].mot.as_str()) {
            def.texte = Some(text.to_string());
        }
    }
    Ok(grille.missing().len())
}

// ---- Rendu ----

/// Triangle plein de sommets donnés (remplissage par lignes).
fn triangle(img: &mut GrayImage, pts: [(f64, f64); 3]) {
    let (min_y, max_y) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    for y in min_y.floor() as i64..=max_y.ceil() as i64 {
        let yc = y as f64 + 0.5;
        let mut xs = Vec::new();
        for i in 0..3 {
            let (a, b) = (pts[i], pts[(i + 1) % 3]);
            if (a.1 <= yc && b.1 > yc) || (b.1 <= yc && a.1 > yc) {
                xs.push(a.0 + (yc - a.1) * (b.0 - a.0) / (b.1 - a.1));
            }
        }
        if let [x0, x1] = xs[..] {
            let (l, r) = (x0.min(x1), x0.max(x1));
            draw::fill_rect(img, l.round() as i64, y, (r - l).round().max(1.0) as i64, 1);
        }
    }
}

fn render(g: &Grille, solution: bool) -> GrayImage {
    let (cols, rows) = (g.largeur as i64, g.hauteur as i64);
    let cell = ((512 - 8) / cols).min(72);
    let (gw, gh) = (cell * cols, cell * rows);
    let (ox, oy) = ((512 - gw) / 2, 4);
    let mut img = draw::canvas((gh + 2 * oy) as u32);
    let at = |r: usize, c: usize| (ox + c as i64 * cell, oy + r as i64 * cell);

    // Cases définitions : noires, numéros en blanc (un ou deux, de haut en bas).
    let digits = if cell >= 64 { 3 } else { 2 };
    for r in 0..g.hauteur {
        for c in 0..g.largeur {
            let (x, y) = at(r, c);
            if g.is_block(r, c) {
                draw::fill_rect(&mut img, x, y, cell, cell);
                let mut here: Vec<&Def> = g.definitions.iter().filter(|d| d.ligne == r && d.colonne == c).collect();
                here.sort_by_key(|d| d.numero);
                let slots = here.len().max(1) as i64;
                for (i, d) in here.iter().enumerate() {
                    let (nw, nh) = (draw::number_width(d.numero, digits), draw::DIGIT_H * digits);
                    let cy = y + (cell * (2 * i as i64 + 1)) / (2 * slots);
                    draw::number_ink(&mut img, d.numero, x + (cell - nw) / 2, cy - nh / 2, digits, draw::WHITE);
                }
                if here.len() == 2 {
                    draw::fill_rect_with(&mut img, x + cell / 5, y + cell / 2, cell * 3 / 5, 1, draw::WHITE);
                }
            } else if solution {
                let ch = g.grille[r].as_bytes()[c];
                let scale = (cell * 9 / 20 / draw::DIGIT_H).max(1);
                let (lw, lh) = (draw::DIGIT_W * scale, draw::DIGIT_H * scale);
                draw::letter(&mut img, ch, x + (cell - lw) / 2, y + (cell - lh) / 2, scale, draw::BLACK);
            }
        }
    }

    // Quadrillage, cadre plus épais.
    for k in 0..=cols {
        let w = if k == 0 || k == cols { 3 } else { 2 };
        draw::fill_rect(&mut img, ox + k * cell - w / 2, oy, w, gh);
    }
    for k in 0..=rows {
        let w = if k == 0 || k == rows { 3 } else { 2 };
        draw::fill_rect(&mut img, ox, oy + k * cell - w / 2, gw, w);
    }

    // Flèches de départ, dans la première case du mot, contre la case définition.
    if !solution {
        let a = (cell as f64 / 5.5).max(6.0);
        for d in &g.definitions {
            let word = &g.mots[d.mot];
            let (x, y) = at(word.ligne, word.colonne);
            let (x, y, s) = (x as f64, y as f64, cell as f64);
            match d.fleche.as_str() {
                "droite" => triangle(&mut img, [(x + 2.0, y + s / 2.0 - a / 2.0), (x + 2.0, y + s / 2.0 + a / 2.0), (x + 2.0 + a * 0.8, y + s / 2.0)]),
                "bas" => triangle(&mut img, [(x + s / 2.0 - a / 2.0, y + 2.0), (x + s / 2.0 + a / 2.0, y + 2.0), (x + s / 2.0, y + 2.0 + a * 0.8)]),
                // Coudée : part vers la droite depuis la case de gauche, puis descend.
                "droite-bas" => {
                    let (sx, sy) = (x + 2.0, y + s / 4.0);
                    draw::fill_rect(&mut img, sx as i64, sy as i64 - 1, (a * 1.2) as i64, 2);
                    let tx = sx + a * 1.2;
                    triangle(&mut img, [(tx - a / 2.0, sy), (tx + a / 2.0, sy), (tx, sy + a * 0.8)]);
                }
                // Coudée : descend depuis la case du dessus, puis part vers la droite.
                "bas-droite" => {
                    let (sx, sy) = (x + s / 4.0, y + 2.0);
                    draw::fill_rect(&mut img, sx as i64 - 1, sy as i64, 2, (a * 1.2) as i64);
                    let ty = sy + a * 1.2;
                    triangle(&mut img, [(sx, ty - a / 2.0), (sx, ty + a / 2.0), (sx + a * 0.8, ty)]);
                }
                _ => {}
            }
        }
    }
    img
}

pub struct Options {
    pub size: Size,
    pub level: u8,
    pub seed: Option<u64>,
    pub solution: bool,
}

pub fn build(ctx: &Ctx, options: &Options) -> Result<Doc> {
    let level = options.level.clamp(1, 4);
    let seed = options.seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
    let (width, height) = options.size.dims();
    let key = format!("mots-fleches-v2-{width}x{height}-n{level}-{seed}");
    let cache = ctx.cache.as_ref();

    let cached: Option<Saved> = cache.filter(|_| !ctx.refresh).and_then(|c| c.get(&key));
    let from_cache = cached.is_some();
    let Saved { mut grille, mut claude_tried } = match cached {
        Some(saved) => saved,
        None => Saved { grille: generate(width, height, level, seed)?, claude_tried: false },
    };
    // Définitions manquantes : Claude, une seule fois, à l'impression (l'aperçu ne coûte rien).
    let mut note = None;
    let mut changed = !from_cache;
    if !grille.missing().is_empty() && !claude_tried && !ctx.preview && !options.solution {
        match complete(&mut grille, level) {
            Ok(_) => claude_tried = true,
            Err(e) => note = Some(format!("Définitions indisponibles ({e}) : la longueur est indiquée.")),
        }
        changed = true;
    }
    if claude_tried && !grille.missing().is_empty() && !options.solution {
        let n = grille.missing().len();
        note = Some(format!("{n} définition(s) n'ont pas pu être rédigées : la longueur est indiquée."));
    }
    if let Some(cache) = cache
        && changed
    {
        cache.put(&key, &Saved { grille: grille.clone(), claude_tried });
    }

    let mut doc = Doc::new();
    doc.from_cache = from_cache && grille.missing().is_empty();
    if options.solution {
        doc.header("Solution des mots fléchés");
    } else {
        doc.header(&format!("Mots fléchés · {}", level_label(level)));
        doc.text("Chaque numéro renvoie à sa définition ; la flèche montre où part le mot.", Style::default().small().center());
    }
    doc.feed(1);
    doc.image(render(&grille, options.solution));
    if !options.solution {
        doc.feed(1);
        let mut defs: Vec<&Def> = grille.definitions.iter().collect();
        defs.sort_by_key(|d| d.numero);
        let width = defs.last().map_or(1, |d| d.numero.to_string().len());
        for d in defs {
            let text = match &d.texte {
                Some(t) => t.clone(),
                None => format!("({} lettres)", grille.mots[d.mot].longueur),
            };
            doc.hanging(&format!("{:>width$}. ", d.numero), &text, Style::default());
        }
        if ctx.preview && !grille.missing().is_empty() {
            doc.feed(1);
            doc.text("Les définitions manquantes seront rédigées par Claude à l'impression.", Style::default().small().center());
        }
        if let Some(note) = note {
            doc.feed(1);
            doc.text(&note, Style::default().small().center());
        }
    }
    doc.text(
        &format!("n° {seed} · grille {} · niveau {level}", options.size.label()),
        Style::default().small().align(Align::Right),
    );
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Op;

    /// L'exemple de la documentation de CrossWords (`docs/format-json.md`).
    const SAMPLE: &str = r####"{
      "format": "crosswords", "version": 1, "type": "fleches", "largeur": 6, "hauteur": 6, "graine": 2, "niveau": 1,
      "grille": ["#C#G#D", "PAGE#E", "###NON", "PAPIER", "#CRAIE", "REELLE"],
      "mots": [
        {"id": 0, "mot": "PAGE", "sens": "horizontal", "ligne": 1, "colonne": 0, "longueur": 4},
        {"id": 1, "mot": "NON", "sens": "horizontal", "ligne": 2, "colonne": 3, "longueur": 3},
        {"id": 2, "mot": "PAPIER", "sens": "horizontal", "ligne": 3, "colonne": 0, "longueur": 6},
        {"id": 3, "mot": "CRAIE", "sens": "horizontal", "ligne": 4, "colonne": 1, "longueur": 5},
        {"id": 4, "mot": "REELLE", "sens": "horizontal", "ligne": 5, "colonne": 0, "longueur": 6},
        {"id": 5, "mot": "CA", "sens": "vertical", "ligne": 0, "colonne": 1, "longueur": 2},
        {"id": 6, "mot": "ACE", "sens": "vertical", "ligne": 3, "colonne": 1, "longueur": 3},
        {"id": 7, "mot": "PRE", "sens": "vertical", "ligne": 3, "colonne": 2, "longueur": 3},
        {"id": 8, "mot": "GENIAL", "sens": "vertical", "ligne": 0, "colonne": 3, "longueur": 6},
        {"id": 9, "mot": "OEIL", "sens": "vertical", "ligne": 2, "colonne": 4, "longueur": 4},
        {"id": 10, "mot": "DENREE", "sens": "vertical", "ligne": 0, "colonne": 5, "longueur": 6}
      ],
      "definitions": [
        {"numero": 1, "ligne": 0, "colonne": 0, "fleche": "droite-bas", "mot": 5, "texte": "Cela, à l'oral"},
        {"numero": 2, "ligne": 0, "colonne": 0, "fleche": "bas-droite", "mot": 0, "texte": "Feuillet d'un livre"},
        {"numero": 3, "ligne": 0, "colonne": 2, "fleche": "droite-bas", "mot": 8, "texte": "Brillant"},
        {"numero": 4, "ligne": 0, "colonne": 4, "fleche": "droite-bas", "mot": 10, "texte": "Produit alimentaire"},
        {"numero": 5, "ligne": 1, "colonne": 4, "fleche": "bas", "mot": 9, "texte": "Organe de la vue"},
        {"numero": 6, "ligne": 2, "colonne": 0, "fleche": "bas-droite", "mot": 2, "texte": "Feuille à écrire"},
        {"numero": 7, "ligne": 2, "colonne": 1, "fleche": "bas", "mot": 6, "texte": "As de carte"},
        {"numero": 8, "ligne": 2, "colonne": 2, "fleche": "droite", "mot": 1, "texte": "Contraire de oui"},
        {"numero": 9, "ligne": 2, "colonne": 2, "fleche": "bas", "mot": 7, "texte": "Champ d'herbe"},
        {"numero": 10, "ligne": 4, "colonne": 0, "fleche": "droite", "mot": 3, "texte": null},
        {"numero": 11, "ligne": 4, "colonne": 0, "fleche": "bas-droite", "mot": 4, "texte": "Vraie"}
      ]
    }"####;

    #[test]
    fn reads_the_documented_format() {
        let g = parse(SAMPLE).unwrap();
        assert_eq!((g.largeur, g.hauteur, g.definitions.len()), (6, 6, 11));
        assert!(g.is_block(0, 0) && !g.is_block(1, 0));
        assert_eq!(g.missing(), ["CRAIE"]);
    }

    #[test]
    fn unknown_format_versions_are_refused() {
        let other = SAMPLE.replace("\"version\": 1", "\"version\": 2");
        assert!(parse(&other).unwrap_err().to_string().contains("version 2"));
        let croises = SAMPLE.replace("\"fleches\"", "\"croises\"");
        assert!(parse(&croises).is_err());
    }

    #[test]
    fn grid_renders_blocks_numbers_and_letters() {
        let g = parse(SAMPLE).unwrap();
        let blank = render(&g, false);
        let filled = render(&g, true);
        assert_eq!(blank.width(), 512);
        let ink = |img: &GrayImage| img.pixels().filter(|p| p.0[0] == 0).count();
        // La solution ajoute les lettres ; la grille vierge a ses flèches.
        assert!(ink(&filled) > ink(&blank) / 2);
        assert!(ink(&blank) > 0);
    }

    #[test]
    fn real_grid_from_the_library_round_trips() {
        // Petit dictionnaire : pas de téléchargement, mais la vraie bibliothèque et le vrai JSON.
        let dict = Dictionary::from_words(["ABC", "DEF", "GHI", "ADG", "BEH", "CFI", "AB", "DE", "GH", "AD", "BE", "CF"]);
        let mut opts = FlechesOptions::new(4, 4);
        opts.attempts = 20; // un dictionnaire minuscule : inutile d'insister
        if let Ok((fl, _)) = generate_fleches(&dict, &opts) {
            let g = parse(&fl.to_json(None, Some(1), Some(1))).unwrap();
            assert!(g.definitions.iter().all(|d| d.texte.is_none()));
            let _ = render(&g, false);
        }
    }

    #[test]
    fn missing_definitions_show_their_length() {
        let g = parse(SAMPLE).unwrap();
        let mut doc = Doc::new();
        for d in &g.definitions {
            let text = d.texte.clone().unwrap_or_else(|| format!("({} lettres)", g.mots[d.mot].longueur));
            doc.hanging(&format!("{:>2}. ", d.numero), &text, Style::default());
        }
        let lines: Vec<&str> = doc
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Line { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(lines.contains(&"10. (5 lettres)"));
    }
}
