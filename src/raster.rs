use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result};
use escpos::driver::Driver;
use escpos::printer::Printer;
use escpos::utils::{BitImageOption, BitImageSize};
use image::imageops::{self, BiLevel, FilterType};
use image::{DynamicImage, GrayImage, ImageFormat, Luma};
use serde::{Deserialize, Serialize};

/// Largeur imprimable de la TM-T88V sur papier 80 mm, en points.
pub const PRINT_WIDTH: u32 = 512;

/// Largeur du papier en points : 512 (TM-T88), ou 576 avec `PRINTR_PAPER_WIDTH=576` (la plupart
/// des imprimantes 80 mm à 203 dpi). Les dessins gardent `PRINT_WIDTH` et sont centrés.
pub fn paper_dots() -> u32 {
    static DOTS: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *DOTS.get_or_init(|| if std::env::var("PRINTR_PAPER_WIDTH").is_ok_and(|v| v.trim() == "576") { 576 } else { 512 })
}

/// Hauteur des bandes envoyées en `GS v 0`, pour ne pas saturer le tampon de l'imprimante.
const STRIP_HEIGHT: u32 = 256;

/// Algorithme de tramage, pour passer des gris au noir et blanc de l'imprimante.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Dither {
    /// Diffusion d'erreur Floyd–Steinberg : dégradés doux, le meilleur choix pour les photos.
    #[default]
    FloydSteinberg,
    /// Diffusion d'erreur Atkinson (Mac d'origine) : plus contrasté, aplats plus nets.
    Atkinson,
    /// Trame ordonnée de Bayer 8×8 : motif régulier, sans grain.
    Bayer,
    /// Simple seuil noir/blanc, pour les logos et les dessins au trait.
    None,
}

/// Décode une image en bornant ses dimensions et la mémoire utilisée : une petite image
/// compressée peut annoncer des millions de pixels et épuiser la mémoire du Pi.
pub fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().context("image illisible")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(10_000);
    limits.max_image_height = Some(10_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().context("image illisible ou trop grande")
}

/// Prépare une image pour l'impression : fond blanc sous la transparence,
/// réduction à la largeur imprimable, centrage, puis noir et blanc selon `dither`.
pub fn prepare(img: &DynamicImage, dither: Dither) -> GrayImage {
    let img =
        if img.width() > PRINT_WIDTH { img.resize(PRINT_WIDTH, u32::MAX, FilterType::Lanczos3) } else { img.clone() };

    // Compose sur fond blanc pleine largeur, image centrée.
    let rgba = img.to_rgba8();
    let offset = (PRINT_WIDTH - rgba.width()) / 2;
    let mut gray = GrayImage::from_pixel(PRINT_WIDTH, rgba.height(), Luma([255]));
    for (x, y, p) in rgba.enumerate_pixels() {
        let [r, g, b, a] = p.0.map(u32::from);
        let luma = (299 * r + 587 * g + 114 * b) / 1000;
        let blended = (luma * a + 255 * (255 - a)) / 255;
        gray.put_pixel(x + offset, y, Luma([blended as u8]));
    }

    match dither {
        Dither::FloydSteinberg => imageops::dither(&mut gray, &BiLevel),
        Dither::Atkinson => atkinson(&mut gray),
        Dither::Bayer => bayer(&mut gray),
        Dither::None => {
            for p in gray.pixels_mut() {
                p.0[0] = if p.0[0] < 128 { 0 } else { 255 };
            }
        }
    }
    gray
}

/// Atkinson : un huitième de l'erreur à six voisins, les deux huitièmes restants sont perdus,
/// d'où des noirs et des blancs plus francs qu'avec Floyd–Steinberg.
fn atkinson(gray: &mut GrayImage) {
    let (w, h) = (gray.width() as i64, gray.height() as i64);
    let mut buf: Vec<i32> = gray.pixels().map(|p| i32::from(p.0[0])).collect();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let old = buf[i].clamp(0, 255);
            let new = if old < 128 { 0 } else { 255 };
            buf[i] = new;
            let err = (old - new) / 8;
            for (dx, dy) in [(1, 0), (2, 0), (-1, 1), (0, 1), (1, 1), (0, 2)] {
                let (nx, ny) = (x + dx, y + dy);
                if (0..w).contains(&nx) && (0..h).contains(&ny) {
                    buf[(ny * w + nx) as usize] += err;
                }
            }
        }
    }
    for (p, v) in gray.pixels_mut().zip(buf) {
        p.0[0] = v as u8;
    }
}

/// Bayer 8×8 : chaque pixel est comparé à un seuil qui dépend de sa position dans la matrice.
fn bayer(gray: &mut GrayImage) {
    const M: [[u8; 8]; 8] = [
        [0, 32, 8, 40, 2, 34, 10, 42],
        [48, 16, 56, 24, 50, 18, 58, 26],
        [12, 44, 4, 36, 14, 46, 6, 38],
        [60, 28, 52, 20, 62, 30, 54, 22],
        [3, 35, 11, 43, 1, 33, 9, 41],
        [51, 19, 59, 27, 49, 17, 57, 25],
        [15, 47, 7, 39, 13, 45, 5, 37],
        [63, 31, 55, 23, 61, 29, 53, 21],
    ];
    for (x, y, p) in gray.enumerate_pixels_mut() {
        let threshold = (u32::from(M[(y % 8) as usize][(x % 8) as usize]) * 4 + 2) as u8;
        p.0[0] = if p.0[0] > threshold { 255 } else { 0 };
    }
}

/// Charge une image depuis un fichier et la prépare pour l'impression.
pub fn load(path: &Path, dither: Dither) -> Result<GrayImage> {
    let bytes = std::fs::read(path).with_context(|| format!("impossible de lire {}", path.display()))?;
    Ok(prepare(&decode(&bytes)?, dither))
}

/// Envoie une image déjà préparée (largeur `PRINT_WIDTH`), découpée en bandes raster.
pub fn print_gray<D: Driver>(printer: &mut Printer<D>, gray: &GrayImage) -> Result<()> {
    let paper = paper_dots();
    let centered;
    let gray = if gray.width() < paper {
        let mut full = GrayImage::from_pixel(paper, gray.height(), Luma([255]));
        imageops::replace(&mut full, gray, i64::from((paper - gray.width()) / 2), 0);
        centered = full;
        &centered
    } else {
        gray
    };
    let mut y = 0;
    while y < gray.height() {
        let h = STRIP_HEIGHT.min(gray.height() - y);
        let strip = imageops::crop_imm(gray, 0, y, gray.width(), h).to_image();
        let mut png = Vec::new();
        DynamicImage::ImageLuma8(strip).write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
        // BitImageOption n'est pas Clone : on en recrée une par bande.
        let option = BitImageOption::new(None, None, BitImageSize::Normal)?;
        printer.bit_image_from_bytes_option(&png, option)?;
        y += h;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn is_bilevel(img: &GrayImage) -> bool {
        img.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255)
    }

    #[test]
    fn large_image_is_scaled_to_print_width() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1024, 600, Rgba([90, 90, 90, 255])));
        let out = prepare(&img, Dither::FloydSteinberg);
        assert_eq!(out.dimensions(), (PRINT_WIDTH, 300));
        assert!(is_bilevel(&out));
    }

    #[test]
    fn small_image_is_centered_on_white() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 10, Rgba([0, 0, 0, 255])));
        let out = prepare(&img, Dither::None);
        assert_eq!(out.dimensions(), (PRINT_WIDTH, 10));
        let offset = (PRINT_WIDTH - 100) / 2;
        assert_eq!(out.get_pixel(offset - 1, 5).0[0], 255);
        assert_eq!(out.get_pixel(offset, 5).0[0], 0);
        assert_eq!(out.get_pixel(offset + 99, 5).0[0], 0);
        assert_eq!(out.get_pixel(offset + 100, 5).0[0], 255);
    }

    #[test]
    fn transparency_becomes_white() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(512, 8, Rgba([0, 0, 0, 0])));
        let out = prepare(&img, Dither::FloydSteinberg);
        assert!(out.pixels().all(|p| p.0[0] == 255));
    }

    fn black_ratio(img: &GrayImage) -> f64 {
        img.pixels().filter(|p| p.0[0] == 0).count() as f64 / f64::from(img.width() * img.height())
    }

    #[test]
    fn every_algorithm_keeps_mid_gray_density() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(512, 64, Rgba([128, 128, 128, 255])));
        for dither in [Dither::FloydSteinberg, Dither::Atkinson, Dither::Bayer] {
            let out = prepare(&img, dither);
            assert!(is_bilevel(&out), "{dither:?}");
            let ratio = black_ratio(&out);
            assert!((0.4..0.6).contains(&ratio), "{dither:?} : ratio de noir = {ratio}");
        }
    }

    #[test]
    fn every_algorithm_follows_the_gray_level() {
        let light = DynamicImage::ImageRgba8(RgbaImage::from_pixel(512, 64, Rgba([200, 200, 200, 255])));
        let dark = DynamicImage::ImageRgba8(RgbaImage::from_pixel(512, 64, Rgba([60, 60, 60, 255])));
        for dither in [Dither::FloydSteinberg, Dither::Atkinson, Dither::Bayer] {
            assert!(black_ratio(&prepare(&light, dither)) < black_ratio(&prepare(&dark, dither)), "{dither:?}");
        }
    }

    #[test]
    fn atkinson_is_more_contrasted_than_floyd_steinberg() {
        // Atkinson perd un quart de l'erreur : un gris très clair finit presque blanc.
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(512, 64, Rgba([235, 235, 235, 255])));
        assert!(black_ratio(&prepare(&img, Dither::Atkinson)) < black_ratio(&prepare(&img, Dither::FloydSteinberg)));
    }

    #[test]
    fn dither_names_in_json() {
        let parsed: Vec<Dither> = serde_json::from_str(r#"["floyd-steinberg", "atkinson", "bayer", "none"]"#).unwrap();
        assert_eq!(parsed, [Dither::FloydSteinberg, Dither::Atkinson, Dither::Bayer, Dither::None]);
    }
}
