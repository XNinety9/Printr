//! Coloriage, par catégorie et par niveau.
//!
//! - Mandala : tracé au trait, différent à chaque numéro. Des couronnes de pétales, de festons et
//!   de perles, répétées par symétrie autour du centre. Au niveau 2 (et sans niveau), le mandala
//!   historique : un ancien ticket (`{"type": "coloring", "seed": 42}`) imprime toujours le même
//!   dessin. Au niveau 1, moins de couronnes, plus larges, au trait épais ; au niveau 3, plus de
//!   secteurs et de couronnes, plus fines, au trait fin.
//! - Noël, Animaux, Anniversaire, Véhicules, Contes et magie : des dessins tirés des planches de
//!   `assets/coloring/`, intégrées au binaire et décrites par `planches.json` (catégorie, niveau,
//!   taille des cases, nombre de dessins). Chaque planche est une grille régulière de dessins déjà
//!   prêts à imprimer : il suffit d'en découper la case. Les numéros se suivent d'une planche à
//!   l'autre, dans l'ordre du fichier.

use std::f64::consts::PI;
use std::sync::OnceLock;

use image::{GrayImage, Luma};
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};
use serde::Deserialize;

use crate::doc::{Align, Doc, Style as TextStyle};
use crate::i18n::t;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/coloring.rs"));
}

const SIZE: u32 = 512;

/// Niveau du coloriage. Sans niveau (anciens tickets) : le niveau 2, dont le mandala est le
/// mandala historique. L'ordre des variantes est celui de la difficulté.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Tous les niveaux, tirés au hasard (le même pour le même numéro).
    #[serde(alias = "tous", alias = "tous_niveaux")]
    All,
    /// 3–6 ans : de grandes zones, un trait épais.
    #[serde(rename = "level_1", alias = "niveau_1", alias = "1")]
    Level1,
    /// 5–8 ans : plus de détails ; le mandala historique.
    #[default]
    #[serde(rename = "level_2", alias = "niveau_2", alias = "2")]
    Level2,
    /// 8 ans et plus : de grands dessins très détaillés.
    #[serde(rename = "level_3", alias = "niveau_3", alias = "3")]
    Level3,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::All => t("tous niveaux", "all levels"),
            Level::Level1 => t("niveau 1", "level 1"),
            Level::Level2 => t("niveau 2", "level 2"),
            Level::Level3 => t("niveau 3", "level 3"),
        }
    }
}

/// Catégorie du coloriage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Toutes les catégories, tirées au hasard (la même pour le même numéro).
    #[serde(alias = "toutes", alias = "tous", alias = "toutes_categories")]
    All,
    #[default]
    Mandala,
    #[serde(alias = "noel", alias = "noël")]
    Christmas,
    #[serde(alias = "animaux")]
    Animals,
    #[serde(alias = "anniversaire")]
    Birthday,
    #[serde(alias = "vehicules", alias = "véhicules")]
    Vehicles,
    #[serde(alias = "contes", alias = "contes_et_magie", alias = "magie")]
    FairyTales,
}

impl Category {
    /// Les catégories précises.
    const EACH: [Category; 6] = [
        Category::Mandala,
        Category::Christmas,
        Category::Animals,
        Category::Birthday,
        Category::Vehicles,
        Category::FairyTales,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::All => t("toutes catégories", "all categories"),
            Category::Mandala => "mandala",
            Category::Christmas => t("Noël", "Christmas"),
            Category::Animals => t("animaux", "animals"),
            Category::Birthday => t("anniversaire", "birthday"),
            Category::Vehicles => t("véhicules", "vehicles"),
            Category::FairyTales => t("contes et magie", "fairy tales and magic"),
        }
    }
}

/// Une planche de `assets/coloring/planches.json`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sheet {
    /// Fichier PNG, dans `assets/coloring/`.
    file: String,
    category: Category,
    level: Level,
    /// Nombre de cases par ligne.
    columns: u32,
    /// Taille d'une case, en points.
    tile_width: u32,
    tile_height: u32,
    /// Nombre de dessins (les dernières cases peuvent être vides).
    count: u32,
}

impl Sheet {
    fn png(&self) -> &'static [u8] {
        embedded::SHEETS
            .iter()
            .find(|(name, _)| *name == self.file)
            .map(|(_, png)| *png)
            .unwrap_or_else(|| panic!("planche absente de assets/coloring/ : {}", self.file))
    }

    /// La planche décodée.
    fn decode(&self) -> GrayImage {
        image::load_from_memory(self.png()).expect("planche intégrée au binaire").to_luma8()
    }

    /// Le dessin n° `n` (à partir de 0) de la planche décodée, sans ses marges blanches.
    fn tile(&self, sheet: &GrayImage, n: u32) -> GrayImage {
        let (x, y) = ((n % self.columns) * self.tile_width, (n / self.columns) * self.tile_height);
        trim(&image::imageops::crop_imm(sheet, x, y, self.tile_width, self.tile_height).to_image())
    }
}

/// Les planches, lues une fois dans `planches.json`.
fn sheets() -> &'static [Sheet] {
    #[derive(Deserialize)]
    struct File {
        sheets: Vec<Sheet>,
    }
    static SHEETS: OnceLock<Vec<Sheet>> = OnceLock::new();
    SHEETS.get_or_init(|| {
        let json = include_str!("../../assets/coloring/planches.json");
        serde_json::from_str::<File>(json).expect("assets/coloring/planches.json").sheets
    })
}

/// Les planches de la catégorie et du niveau (tous niveaux : toutes celles de la catégorie).
fn sheets_for(category: Category, level: Level) -> Vec<&'static Sheet> {
    let of_category: Vec<_> = sheets().iter().filter(|s| s.category == category).collect();
    if level == Level::All {
        return of_category;
    }
    let chosen = closest_level(of_category.iter().map(|s| s.level), level);
    of_category.into_iter().filter(|s| Some(s.level) == chosen).collect()
}

/// Le niveau à servir parmi ceux qui existent : celui demandé, sinon le plus proche en dessous (le
/// niveau 2 pour un niveau 3 qu'une catégorie n'a pas encore), sinon le plus simple.
fn closest_level(available: impl Iterator<Item = Level> + Clone, wanted: Level) -> Option<Level> {
    available.clone().filter(|&l| l <= wanted).max().or_else(|| available.min())
}

/// Recadre l'image sur son encre.
fn trim(img: &GrayImage) -> GrayImage {
    let ink = |x: u32, y: u32| img.get_pixel(x, y).0[0] < 128;
    let (w, h) = img.dimensions();
    let rows: Vec<u32> = (0..h).filter(|&y| (0..w).any(|x| ink(x, y))).collect();
    let cols: Vec<u32> = (0..w).filter(|&x| (0..h).any(|y| ink(x, y))).collect();
    match (rows.first(), rows.last(), cols.first(), cols.last()) {
        (Some(&y0), Some(&y1), Some(&x0), Some(&x1)) => {
            image::imageops::crop_imm(img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image()
        }
        _ => img.clone(),
    }
}

#[derive(Clone, Copy)]
enum Motif {
    /// Cercle centré.
    Ring { r: f64 },
    /// Pétale en amande entre deux rayons, de demi-largeur `w`.
    Petal { r0: f64, r1: f64, w: f64 },
    /// Cercle posé sur la couronne.
    Bead { r: f64, size: f64 },
    /// Demi-cercles accrochés à l'extérieur d'un anneau.
    Scallop { r: f64, size: f64 },
}

struct Layer {
    motif: Motif,
    /// Décalage d'un demi-secteur : les motifs se glissent entre ceux de la couronne précédente.
    shifted: bool,
}

/// Distance signée à un cercle (négative dedans).
fn circle(x: f64, y: f64, cx: f64, cy: f64, r: f64) -> f64 {
    ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r
}

impl Motif {
    /// Distance signée au motif, dans le repère local d'un secteur (le motif est centré sur l'axe x).
    fn distance(self, x: f64, y: f64, r: f64) -> f64 {
        match self {
            Motif::Ring { r: radius } => r - radius,
            Motif::Bead { r: radius, size } => circle(x, y, radius, 0.0, size),
            Motif::Scallop { r: radius, size } => {
                let d = circle(x, y, radius, 0.0, size);
                if r < radius { d.abs().max(radius - r) } else { d }
            }
            Motif::Petal { r0, r1, w } => {
                // Intersection de deux disques : une amande de longueur r1 - r0 et de largeur 2w.
                let half = (r1 - r0) / 2.0;
                let radius = (half * half + w * w) / (2.0 * w);
                let mid = (r0 + r1) / 2.0;
                circle(x, y, mid, -(radius - w), radius).max(circle(x, y, mid, radius - w, radius))
            }
        }
    }
}

/// Style du mandala, selon le niveau.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MandalaStyle {
    /// Niveau 1 : moins de secteurs, des couronnes larges, un trait épais.
    Simple,
    /// Niveau 2 : le mandala historique. Ses tirages ne doivent jamais changer.
    Classic,
    /// Niveau 3 : plus de secteurs, des couronnes fines et nombreuses, un trait fin.
    Dense,
}

impl MandalaStyle {
    const EACH: [MandalaStyle; 3] = [MandalaStyle::Simple, MandalaStyle::Classic, MandalaStyle::Dense];

    /// Nombres de secteurs possibles (une valeur répétée sort plus souvent).
    fn folds(self) -> &'static [usize] {
        match self {
            MandalaStyle::Simple => &[6, 8, 8, 10],
            MandalaStyle::Classic => &[8, 10, 12, 12, 16],
            MandalaStyle::Dense => &[12, 16, 16, 20, 24],
        }
    }

    /// Largeur d'une couronne, en points.
    fn bands(self) -> std::ops::Range<f64> {
        match self {
            MandalaStyle::Simple => 44.0..64.0,
            MandalaStyle::Classic => 22.0..44.0,
            MandalaStyle::Dense => 14.0..26.0,
        }
    }

    /// Demi-épaisseur du trait, en points.
    fn line(self) -> f64 {
        match self {
            MandalaStyle::Simple => 2.2,
            MandalaStyle::Classic => 1.3,
            MandalaStyle::Dense => 1.0,
        }
    }
}

/// Le mandala : le nombre de secteurs et ses couronnes.
fn design(rng: &mut StdRng, style: MandalaStyle) -> (usize, Vec<Layer>) {
    let folds = *style.folds().choose(rng).expect("symétries");
    let bands = style.bands();
    let sector = 2.0 * PI / folds as f64;
    let mut layers = vec![
        Layer { motif: Motif::Ring { r: 14.0 }, shifted: false },
        Layer { motif: Motif::Bead { r: 0.0, size: 4.0 }, shifted: false },
    ];
    let mut r = 14.0;
    if style == MandalaStyle::Dense {
        // Un cœur plus large : avec beaucoup de secteurs, les motifs collés au centre se
        // confondraient en une tache noire.
        r = 26.0;
        layers.push(Layer { motif: Motif::Ring { r }, shifted: false });
    }
    let mut shifted = false;
    while r < 215.0 {
        let band: f64 = rng.random_range(bands.clone()).min(244.0 - r);
        // Largeur d'un secteur à mi-couronne : les motifs ne débordent pas sur leurs voisins.
        let room = (r + band / 2.0) * sector / 2.0;
        if style == MandalaStyle::Dense && room < 7.0 {
            // Trop étroit pour des motifs lisibles : un simple cercle.
            r += band;
            layers.push(Layer { motif: Motif::Ring { r }, shifted: false });
            continue;
        }
        match rng.random_range(0..4) {
            0 => {
                let w = room * rng.random_range(0.55..0.9);
                layers.push(Layer { motif: Motif::Petal { r0: r, r1: r + band, w }, shifted });
                if rng.random_bool(0.6) {
                    // Pétale intérieur, comme une nervure.
                    let inset = band * 0.25;
                    layers.push(Layer {
                        motif: Motif::Petal { r0: r + inset, r1: r + band - inset, w: w * 0.45 },
                        shifted,
                    });
                }
            }
            1 => {
                let size = (band / 2.0).min(room * 0.9);
                layers.push(Layer { motif: Motif::Bead { r: r + band / 2.0, size }, shifted });
                layers.push(Layer { motif: Motif::Bead { r: r + band / 2.0, size: size * 0.4 }, shifted });
            }
            2 => {
                let size = room.min(band * 0.8);
                layers.push(Layer { motif: Motif::Scallop { r, size }, shifted });
                layers.push(Layer { motif: Motif::Bead { r: r + size * 0.45, size: size * 0.25 }, shifted });
            }
            _ => {
                // Pétales longs et fins, deux par secteur.
                let w = room * 0.35;
                layers.push(Layer { motif: Motif::Petal { r0: r, r1: r + band, w }, shifted: false });
                layers.push(Layer { motif: Motif::Petal { r0: r, r1: r + band * 0.7, w: w * 0.8 }, shifted: true });
            }
        }
        r += band;
        if rng.random_bool(0.55) {
            layers.push(Layer { motif: Motif::Ring { r }, shifted: false });
            r += rng.random_range(0.0..8.0);
        }
        shifted = !shifted;
    }
    layers.push(Layer { motif: Motif::Ring { r: 246.0 }, shifted: false });
    (folds, layers)
}

/// `line` : demi-épaisseur du trait, en points.
fn render(folds: usize, layers: &[Layer], line: f64) -> GrayImage {
    let sector = 2.0 * PI / folds as f64;
    let center = SIZE as f64 / 2.0;
    GrayImage::from_fn(SIZE, SIZE, |px, py| {
        let (dx, dy) = (px as f64 + 0.5 - center, py as f64 + 0.5 - center);
        let r = (dx * dx + dy * dy).sqrt();
        if r > 250.0 {
            return Luma([255]);
        }
        let theta = dy.atan2(dx);
        let ink = layers.iter().any(|layer| {
            let phase = if layer.shifted { sector / 2.0 } else { 0.0 };
            let a = (theta - phase).rem_euclid(sector);
            let a = if a > sector / 2.0 { a - sector } else { a };
            let (x, y) = (r * a.cos(), r * a.sin());
            let d = layer.motif.distance(x, y, r);
            // Petites perles pleines, contours partout ailleurs.
            match layer.motif {
                Motif::Bead { size, .. } if size < 4.5 => d < 0.5,
                _ => d.abs() < line,
            }
        });
        Luma([if ink { 0 } else { 255 }])
    })
}

/// Un dessin, centré sur la largeur du papier.
fn centered(img: &GrayImage) -> GrayImage {
    let mut page = GrayImage::from_pixel(SIZE, img.height(), Luma([255]));
    image::imageops::overlay(&mut page, img, (SIZE.saturating_sub(img.width()) / 2) as i64, 0);
    page
}

/// Numéro du dessin, de 1 à `count` : celui demandé (au-delà, on refait le tour), ou au hasard.
fn number(count: u64, seed: Option<u64>) -> u64 {
    seed.map_or_else(|| rand::rng().random_range(1..=count), |n| (n.max(1) - 1) % count + 1)
}

/// Le dessin n° `number` (à partir de 1) parmi les planches, numérotées à la suite.
fn drawing(sheets: &[&Sheet], number: u64) -> GrayImage {
    let mut n = number as u32 - 1;
    for sheet in sheets {
        if n < sheet.count {
            return centered(&sheet.tile(&sheet.decode(), n));
        }
        n -= sheet.count;
    }
    unreachable!("numéro {number} au-delà des planches")
}

/// Choix entre `n` possibilités, fixé par le numéro du dessin (`salt` : un tirage par choix).
/// Indépendant de la séquence du mandala, qui part du numéro lui-même.
fn pick(seed: u64, salt: u64, n: usize) -> usize {
    StdRng::seed_from_u64(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15)).random_range(0..n)
}

/// Le style du mandala pour ce niveau ; tous niveaux : tiré par le numéro.
fn mandala_style(level: Level, seed: u64) -> MandalaStyle {
    match level {
        Level::Level1 => MandalaStyle::Simple,
        Level::Level2 => MandalaStyle::Classic,
        Level::Level3 => MandalaStyle::Dense,
        Level::All => MandalaStyle::EACH[pick(seed, 2, MandalaStyle::EACH.len())],
    }
}

pub fn build(category: Category, level: Level, seed: Option<u64>) -> Doc {
    // Toutes les catégories : le numéro choisit la catégorie, et c'est lui qu'on affiche, pour que
    // le même numéro redonne le même dessin.
    let all = category == Category::All;
    let seed = if all { Some(seed.unwrap_or_else(|| rand::rng().random_range(1..100_000))) } else { seed };
    let category = match seed {
        Some(s) if all => Category::EACH[pick(s, 1, Category::EACH.len())],
        _ => category,
    };
    // Une catégorie sans planche (impossible tant que `levels_pick_their_sheets` passe) retombe
    // sur le mandala plutôt que de paniquer sur un tirage parmi zéro dessin.
    let sheets = if category == Category::Mandala { Vec::new() } else { sheets_for(category, level) };
    let (shown, img) = if sheets.is_empty() {
        let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
        let style = mandala_style(level, seed);
        let (folds, layers) = design(&mut StdRng::seed_from_u64(seed), style);
        (seed, render(folds, &layers, style.line()))
    } else {
        let count = sheets.iter().map(|s| s.count as u64).sum();
        let n = number(count, seed);
        (if all { seed.unwrap_or(n) } else { n }, drawing(&sheets, n))
    };
    let seed = shown;

    let mut doc = Doc::new();
    doc.header(t("Coloriage", "Coloring page"));
    doc.feed(1);
    doc.image(img);
    doc.text(&tr!("n° {seed}", "No. {seed}"), TextStyle::default().small().align(Align::Right));
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mandalas_are_line_art() {
        for seed in [1, 2, 3, 42] {
            for style in MandalaStyle::EACH {
                let (folds, layers) = design(&mut StdRng::seed_from_u64(seed), style);
                let img = render(folds, &layers, style.line());
                let black = img.pixels().filter(|p| p.0[0] == 0).count() as f64 / (SIZE * SIZE) as f64;
                // Assez de traits pour colorier, mais pas une tache noire.
                assert!((0.04..0.3).contains(&black), "{style:?}, graine {seed} : {:.0} % de noir", black * 100.0);
            }
        }
    }

    /// Le mandala d'avant les niveaux, au pixel près : empreintes SHA-256 des dessins du code
    /// d'origine. Sans niveau comme au niveau 2, un ancien ticket imprime toujours le même mandala.
    #[test]
    fn classic_mandala_never_changes() {
        use sha2::{Digest, Sha256};
        let expected = [
            (1, "ded3be4008351ed575bed650c42e63f0cd50fa21acb2bb94179054bf5adc7942"),
            (2, "ba813383c6baf7965c301ad7ac7787e97d81e7fdb340327d9734131899413dfb"),
            (3, "13af8dc66aeede3a6bd59c6007833808db48f6af03542026fd56ad586832dd81"),
            (42, "0d731c21b9597dfd6ff58aed4bc9bc1044b897f2ab85d361de32929a3bbb7658"),
            (2026, "dc17f573769b8400feeff1633b8b142efd1953ed16fc91d5b82bcb9578b37eaa"),
            (99_999, "77dc14cbb88b37c94c89638243c2ac0b1aba60db61af365ca6a1ceef79a6832b"),
        ];
        for (seed, hash) in expected {
            let (folds, layers) = design(&mut StdRng::seed_from_u64(seed), MandalaStyle::Classic);
            let img = render(folds, &layers, MandalaStyle::Classic.line());
            let digest: String = Sha256::digest(img.as_raw()).iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(digest, hash, "graine {seed} : le mandala historique a changé");
        }
        // Le niveau 2 dessine ce mandala-là, et un ticket sans niveau désérialise bien le niveau 2.
        assert_eq!(mandala_style(Level::Level2, 42), MandalaStyle::Classic);
        let block: crate::blocks::Block = serde_json::from_str(r#"{"type": "coloring", "seed": 42}"#).unwrap();
        assert!(matches!(
            block,
            crate::blocks::Block::Coloring { category: Category::Mandala, level: Level::Level2, .. }
        ));
    }

    #[test]
    fn sheets_match_their_metadata() {
        assert!(!sheets().is_empty());
        for sheet in sheets() {
            let img = sheet.decode();
            assert!(
                sheet.columns > 0 && sheet.count > 0,
                "{} : columns et count doivent valoir au moins 1",
                sheet.file
            );
            // L'image a exactement la taille de la grille décrite : une faute dans columns,
            // count ou la taille des cases se voit ici, pas sur un dessin coupé à l'impression.
            let rows = sheet.count.div_ceil(sheet.columns);
            let expected = (sheet.columns * sheet.tile_width, rows * sheet.tile_height);
            assert_eq!(
                img.dimensions(),
                expected,
                "{} : l'image ne correspond pas à planches.json ({} colonnes × {} lignes de cases de {} × {}, \
                 soit {} × {} points)",
                sheet.file,
                sheet.columns,
                rows,
                sheet.tile_width,
                sheet.tile_height,
                expected.0,
                expected.1
            );
            assert!(sheet.tile_width <= SIZE, "{} : cases plus larges que le papier", sheet.file);
            assert_ne!(sheet.category, Category::Mandala, "{}", sheet.file);
            for n in 0..sheet.count {
                let tile = sheet.tile(&img, n);
                // Noir et blanc pur, et du trait.
                assert!(tile.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255), "{} n° {}", sheet.file, n + 1);
                assert!(tile.pixels().any(|p| p.0[0] == 0), "{} n° {} vide", sheet.file, n + 1);
            }
        }
        // Toutes les planches intégrées sont décrites.
        for (name, _) in embedded::SHEETS {
            assert!(sheets().iter().any(|s| s.file == *name), "{name} absente de planches.json");
        }
    }

    #[test]
    fn levels_pick_their_sheets() {
        let count = |c, l| sheets_for(c, l).iter().map(|s| s.count).sum::<u32>();
        // Chaque catégorie dessinée : 50 dessins par niveau, 150 sur tous les niveaux.
        for category in Category::EACH.into_iter().filter(|&c| c != Category::Mandala) {
            for level in [Level::Level1, Level::Level2, Level::Level3] {
                assert_eq!(count(category, level), 50, "{category:?} {level:?}");
                assert!(sheets_for(category, level).iter().all(|s| s.level == level), "{category:?} {level:?}");
            }
            assert_eq!(count(category, Level::All), 150, "{category:?}");
        }
        // Niveau absent d'une catégorie : le plus proche en dessous, sinon le plus simple.
        use Level::*;
        assert_eq!(closest_level([Level1, Level2].into_iter(), Level3), Some(Level2));
        assert_eq!(closest_level([Level1, Level3].into_iter(), Level2), Some(Level1));
        assert_eq!(closest_level([Level2, Level3].into_iter(), Level1), Some(Level2));
        assert_eq!(closest_level([Level3].into_iter(), Level3), Some(Level3));
        assert_eq!(closest_level([].into_iter(), Level2), None);
        // Tous niveaux : le n° 51 est le premier dessin de la planche suivante.
        let all = sheets_for(Category::Animals, Level::All);
        let second = sheets_for(Category::Animals, Level::Level2)[0];
        assert_eq!(drawing(&all, 51), centered(&second.tile(&second.decode(), 0)));
    }

    #[test]
    fn all_categories_and_levels_vary_but_repeat() {
        let png = |category, level, seed| {
            let doc = build(category, level, Some(seed));
            doc.to_json().to_string()
        };
        // Le même numéro redonne le même dessin.
        assert_eq!(png(Category::All, Level::All, 7), png(Category::All, Level::All, 7));
        // Toutes les catégories et tous les niveaux du mandala sortent.
        let categories: std::collections::HashSet<_> = (1..60).map(|s| pick(s, 1, Category::EACH.len())).collect();
        assert_eq!(categories.len(), Category::EACH.len());
        let styles: std::collections::HashSet<_> = (1..60).map(|s| pick(s, 2, MandalaStyle::EACH.len())).collect();
        assert_eq!(styles.len(), MandalaStyle::EACH.len());
        // Tous niveaux : le style tiré par le numéro, celui d'un des niveaux.
        for seed in 1..10 {
            let level = [Level::Level1, Level::Level2, Level::Level3][pick(seed, 2, MandalaStyle::EACH.len())];
            assert_eq!(mandala_style(Level::All, seed), mandala_style(level, seed));
            assert_eq!(png(Category::Mandala, Level::All, seed), png(Category::Mandala, level, seed));
        }
    }

    #[test]
    fn numbers_wrap_around() {
        assert_eq!(number(50, Some(1)), 1);
        assert_eq!(number(50, Some(50)), 50);
        assert_eq!(number(50, Some(51)), 1);
        assert_eq!(number(50, Some(0)), 1);
        assert!((1..=50).contains(&number(50, None)));
    }
}
