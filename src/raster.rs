use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result};
use escpos::driver::Driver;
use escpos::printer::Printer;
use escpos::utils::{BitImageOption, BitImageSize};
use image::imageops::{self, BiLevel, FilterType};
use image::{DynamicImage, GrayImage, ImageFormat, Luma};

/// Largeur imprimable de la TM-T88V sur papier 80 mm, en points.
pub const PRINT_WIDTH: u32 = 512;

/// Hauteur des bandes envoyées en `GS v 0`, pour ne pas saturer le tampon de l'imprimante.
const STRIP_HEIGHT: u32 = 256;

/// Prépare une image pour l'impression : fond blanc sous la transparence,
/// réduction à la largeur imprimable, centrage, puis noir et blanc
/// (tramage Floyd–Steinberg, ou simple seuil si `dither` est faux).
pub fn prepare(img: &DynamicImage, dither: bool) -> GrayImage {
    let img = if img.width() > PRINT_WIDTH {
        img.resize(PRINT_WIDTH, u32::MAX, FilterType::Lanczos3)
    } else {
        img.clone()
    };

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

    if dither {
        imageops::dither(&mut gray, &BiLevel);
    } else {
        for p in gray.pixels_mut() {
            p.0[0] = if p.0[0] < 128 { 0 } else { 255 };
        }
    }
    gray
}

/// Charge une image depuis un fichier et la prépare pour l'impression.
pub fn load(path: &Path, dither: bool) -> Result<GrayImage> {
    let img = image::open(path).with_context(|| format!("impossible de lire {}", path.display()))?;
    Ok(prepare(&img, dither))
}

/// Envoie une image déjà préparée (largeur `PRINT_WIDTH`), découpée en bandes raster.
pub fn print_gray<D: Driver>(printer: &mut Printer<D>, gray: &GrayImage) -> Result<()> {
    let mut y = 0;
    while y < gray.height() {
        let h = STRIP_HEIGHT.min(gray.height() - y);
        let strip = imageops::crop_imm(gray, 0, y, PRINT_WIDTH, h).to_image();
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
        let out = prepare(&img, true);
        assert_eq!(out.dimensions(), (PRINT_WIDTH, 300));
        assert!(is_bilevel(&out));
    }

    #[test]
    fn small_image_is_centered_on_white() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 10, Rgba([0, 0, 0, 255])));
        let out = prepare(&img, false);
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
        let out = prepare(&img, true);
        assert!(out.pixels().all(|p| p.0[0] == 255));
    }

    #[test]
    fn dithering_keeps_mid_gray_density() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(512, 64, Rgba([128, 128, 128, 255])));
        let out = prepare(&img, true);
        let black = out.pixels().filter(|p| p.0[0] == 0).count() as f64;
        let ratio = black / (512.0 * 64.0);
        assert!((0.4..0.6).contains(&ratio), "ratio de noir = {ratio}");
    }
}
