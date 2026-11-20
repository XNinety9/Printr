//! Gadgets : des objets utiles imprimés à la taille réelle (règle, cible, cercle, rapporteur,
//! papier quadrillé, baguier, patron de dé…). Tout se mesure en millimètres, convertis en points
//! selon la résolution de l'imprimante, avec la correction d'avance du papier réglée dans l'appli.

pub mod chord;
pub mod die;
pub mod grid_paper;
pub mod illusion;
pub mod protractor;
pub mod ring_sizer;
pub mod ruler;
pub mod shapes;
pub mod target;

use image::{GrayImage, Luma};

use crate::draw::{self, BLACK, WHITE};
use crate::raster::{self, PRINT_WIDTH};

/// Largeur la plus grande d'un gadget, en mm : elle tient sur toutes les imprimantes (64 mm
/// imprimables au moins), et reste comparable à une règle d'écolier.
pub const MAX_WIDTH_MM: f64 = 60.0;

/// Correction d'avance du papier, en pour mille : 1000 ne corrige rien.
pub const CALIBRATION_RANGE: std::ops::RangeInclusive<u16> = 900..=1100;

/// Points par millimètre, en largeur (tête d'impression) et en longueur (avance du papier).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub x: f64,
    pub y: f64,
}

impl Scale {
    /// La TM-T88V (512 points) imprime à 180 dpi ; les imprimantes à 576 points, à 203 dpi.
    pub fn for_paper(paper_dots: u32, calibration: u16) -> Self {
        let x = if paper_dots >= 576 { 8.0 } else { 180.0 / 25.4 };
        let factor = f64::from(calibration.clamp(*CALIBRATION_RANGE.start(), *CALIBRATION_RANGE.end())) / 1000.0;
        Scale { x, y: x * factor }
    }

    /// L'échelle de l'imprimante branchée, avec la correction enregistrée dans l'appli.
    pub fn current() -> Self {
        let calibration = crate::store::read_data().ok().and_then(|d| d.calibration).unwrap_or(1000);
        Self::for_paper(raster::paper_dots(), calibration)
    }

    /// Largeur dessinable, en mm.
    pub fn width_mm(&self) -> f64 {
        f64::from(PRINT_WIDTH) / self.x
    }

    pub fn dx(&self, mm: f64) -> i64 {
        (mm * self.x).round() as i64
    }

    pub fn dy(&self, mm: f64) -> i64 {
        (mm * self.y).round() as i64
    }

    /// Taille des chiffres dessinés (5×7 points × l'échelle) : environ 3 mm de haut.
    pub fn digits(&self) -> i64 {
        ((3.0 * self.y) / 7.0).round().max(2.0) as i64
    }

    /// Petits chiffres, environ 2 mm de haut.
    pub fn small_digits(&self) -> i64 {
        ((2.0 * self.y) / 7.0).round().max(1.0) as i64
    }
}

/// Pointillés : longueurs, en mm, du trait et du blanc.
#[derive(Clone, Copy)]
pub struct Dash(pub f64, pub f64);

impl Dash {
    pub const CUT: Dash = Dash(2.0, 1.2);
    pub const FOLD: Dash = Dash(1.0, 1.0);

    fn ink(self, along: f64) -> bool {
        along.rem_euclid(self.0 + self.1) < self.0
    }
}

/// Une feuille pleine largeur, où l'on dessine en millimètres (origine en haut à gauche).
pub struct Sheet {
    pub img: GrayImage,
    pub s: Scale,
}

impl Sheet {
    pub fn new(s: Scale, height_mm: f64) -> Self {
        let height = (height_mm * s.y).ceil().max(1.0) as u32;
        Sheet { img: draw::canvas(height), s }
    }

    /// Le point (px, py), au centre du pixel, en mm.
    fn mm(&self, px: i64, py: i64) -> (f64, f64) {
        ((px as f64 + 0.5) / self.s.x, (py as f64 + 0.5) / self.s.y)
    }

    /// Les pixels de la zone (en mm), rognés à l'image.
    fn pixels(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> impl Iterator<Item = (i64, i64)> + use<> {
        let (w, h) = (i64::from(self.img.width()), i64::from(self.img.height()));
        let (px0, px1) = (((x0 * self.s.x).floor() as i64 - 1).max(0), ((x1 * self.s.x).ceil() as i64 + 1).min(w));
        let (py0, py1) = (((y0 * self.s.y).floor() as i64 - 1).max(0), ((y1 * self.s.y).ceil() as i64 + 1).min(h));
        (py0..py1).flat_map(move |py| (px0..px1).map(move |px| (px, py)))
    }

    fn put(&mut self, px: i64, py: i64, ink: Luma<u8>) {
        self.img.put_pixel(px as u32, py as u32, ink);
    }

    /// Trait vertical net de `dots` points de large, centré sur l'abscisse `x` (mm).
    pub fn vline(&mut self, x: f64, y0: f64, y1: f64, dots: i64) {
        let px = self.s.dx(x) - dots / 2;
        let (py0, py1) = (self.s.dy(y0), self.s.dy(y1));
        draw::fill_rect(&mut self.img, px, py0, dots, (py1 - py0).max(1));
    }

    /// Trait horizontal net de `dots` points d'épaisseur, centré sur l'ordonnée `y` (mm).
    pub fn hline(&mut self, y: f64, x0: f64, x1: f64, dots: i64) {
        let py = self.s.dy(y) - dots / 2;
        let (px0, px1) = (self.s.dx(x0), self.s.dx(x1));
        draw::fill_rect(&mut self.img, px0, py, (px1 - px0).max(1), dots);
    }

    /// Segment de `width` mm d'épaisseur, plein ou en pointillés.
    pub fn line(&mut self, (x1, y1): (f64, f64), (x2, y2): (f64, f64), width: f64, dash: Option<Dash>) {
        let half = width / 2.0;
        let (dx, dy) = (x2 - x1, y2 - y1);
        let len2 = (dx * dx + dy * dy).max(1e-9);
        let len = len2.sqrt();
        let pixels: Vec<_> =
            self.pixels(x1.min(x2) - half, y1.min(y2) - half, x1.max(x2) + half, y1.max(y2) + half).collect();
        for (px, py) in pixels {
            let (x, y) = self.mm(px, py);
            let t = (((x - x1) * dx + (y - y1) * dy) / len2).clamp(0.0, 1.0);
            let (cx, cy) = (x1 + t * dx, y1 + t * dy);
            if (x - cx).hypot(y - cy) <= half && dash.is_none_or(|d| d.ink(t * len)) {
                self.put(px, py, BLACK);
            }
        }
    }

    /// Contour d'un polygone fermé.
    pub fn polygon(&mut self, points: &[(f64, f64)], width: f64, dash: Option<Dash>) {
        for (i, &a) in points.iter().enumerate() {
            self.line(a, points[(i + 1) % points.len()], width, dash);
        }
    }

    /// Cercle de rayon `r` (mm), trait de `width` mm, plein ou en pointillés.
    pub fn circle(&mut self, (cx, cy): (f64, f64), r: f64, width: f64, dash: Option<Dash>) {
        let half = width / 2.0;
        let pixels: Vec<_> = self.pixels(cx - r - half, cy - r - half, cx + r + half, cy + r + half).collect();
        for (px, py) in pixels {
            let (x, y) = self.mm(px, py);
            let d = (x - cx).hypot(y - cy);
            if (d - r).abs() <= half && dash.is_none_or(|dash| dash.ink((y - cy).atan2(x - cx) * r)) {
                self.put(px, py, BLACK);
            }
        }
    }

    /// Arc de cercle, entre les angles `from` et `to` (radians, sens horaire depuis la droite,
    /// comme `atan2` avec l'axe des y vers le bas).
    pub fn arc(&mut self, (cx, cy): (f64, f64), r: f64, width: f64, from: f64, to: f64) {
        let half = width / 2.0;
        let pixels: Vec<_> = self.pixels(cx - r - half, cy - r - half, cx + r + half, cy + r + half).collect();
        for (px, py) in pixels {
            let (x, y) = self.mm(px, py);
            let angle = (y - cy).atan2(x - cx);
            if ((x - cx).hypot(y - cy) - r).abs() <= half && (from..=to).contains(&angle) {
                self.put(px, py, BLACK);
            }
        }
    }

    /// Disque plein de rayon `r` (mm), dans la couleur `ink`.
    pub fn disc(&mut self, (cx, cy): (f64, f64), r: f64, ink: Luma<u8>) {
        let pixels: Vec<_> = self.pixels(cx - r, cy - r, cx + r, cy + r).collect();
        for (px, py) in pixels {
            let (x, y) = self.mm(px, py);
            if (x - cx).hypot(y - cy) <= r {
                self.put(px, py, ink);
            }
        }
    }

    /// Peint de `ink` les pixels de la zone (en mm) qui vérifient `inside`.
    pub fn fill_where(
        &mut self,
        (x0, y0, x1, y1): (f64, f64, f64, f64),
        ink: Luma<u8>,
        inside: impl Fn(f64, f64) -> bool,
    ) {
        let pixels: Vec<_> = self.pixels(x0, y0, x1, y1).collect();
        for (px, py) in pixels {
            let (x, y) = self.mm(px, py);
            if inside(x, y) {
                self.put(px, py, ink);
            }
        }
    }

    /// Inverse les pixels de la zone (en mm) qui vérifient `inside`.
    pub fn invert_where(&mut self, (x0, y0, x1, y1): (f64, f64, f64, f64), inside: impl Fn(f64, f64) -> bool) {
        let pixels: Vec<_> = self.pixels(x0, y0, x1, y1).collect();
        for (px, py) in pixels {
            let (x, y) = self.mm(px, py);
            if inside(x, y) {
                let p = self.img.get_pixel(px as u32, py as u32).0[0];
                self.put(px, py, Luma([255 - p]));
            }
        }
    }

    /// Texte en 5×7 (majuscules, chiffres, signes), de `scale` points par pixel de police, centré
    /// sur le point (x, y) en mm.
    pub fn label(&mut self, s: &str, (x, y): (f64, f64), scale: i64, ink: Luma<u8>) {
        let w = draw::text_width(s, scale);
        let h = draw::DIGIT_H * scale;
        draw::text_ink(&mut self.img, s, self.s.dx(x) - w / 2, self.s.dy(y) - h / 2, scale, ink);
    }

    /// Texte aligné à gauche sur `x`, centré verticalement sur `y`.
    pub fn label_left(&mut self, s: &str, (x, y): (f64, f64), scale: i64) {
        let h = draw::DIGIT_H * scale;
        draw::text(&mut self.img, s, self.s.dx(x), self.s.dy(y) - h / 2, scale);
    }

    /// Texte aligné à droite sur `x`, centré verticalement sur `y`.
    pub fn label_right(&mut self, s: &str, (x, y): (f64, f64), scale: i64) {
        let w = draw::text_width(s, scale);
        let h = draw::DIGIT_H * scale;
        draw::text(&mut self.img, s, self.s.dx(x) - w, self.s.dy(y) - h / 2, scale);
    }

    /// Petite croix de centrage de `size` mm.
    pub fn cross(&mut self, (cx, cy): (f64, f64), size: f64) {
        let w = 0.3;
        self.line((cx - size / 2.0, cy), (cx + size / 2.0, cy), w, None);
        self.line((cx, cy - size / 2.0), (cx, cy + size / 2.0), w, None);
    }

    pub fn into_image(self) -> GrayImage {
        self.img
    }
}

/// Encre opposée.
pub fn opposite(ink: Luma<u8>) -> Luma<u8> {
    if ink == BLACK { WHITE } else { BLACK }
}

/// Vérifie qu'une mesure est dans ses bornes, avec un message qui dit lesquelles.
pub fn check_range(what: &str, value: f64, min: f64, max: f64, unit: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        (min..=max).contains(&value),
        "{}",
        tr!("{what} : entre {min} et {max} {unit}", "{what}: between {min} and {max} {unit}")
    );
    Ok(())
}

/// Nombre lisible : « 12 », « 12,5 » (« 12.5 » en anglais).
pub fn fmt_mm(v: f64) -> String {
    let s = if (v - v.round()).abs() < 1e-6 { format!("{}", v.round() as i64) } else { format!("{v:.1}") };
    if crate::i18n::en() { s } else { s.replace('.', ",") }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// TM-T88V, sans correction.
    pub fn tm88() -> Scale {
        Scale::for_paper(512, 1000)
    }

    pub fn black(img: &GrayImage, px: u32, py: u32) -> bool {
        img.get_pixel(px, py).0[0] == 0
    }

    #[test]
    fn scale_follows_the_printer() {
        let s = tm88();
        assert!((s.x - 7.0866).abs() < 1e-3 && s.x == s.y);
        assert!((s.width_mm() - 72.2).abs() < 0.1);
        let wide = Scale::for_paper(576, 1000);
        assert_eq!((wide.x, wide.width_mm()), (8.0, 64.0));
        assert!(MAX_WIDTH_MM <= wide.width_mm());
    }

    #[test]
    fn calibration_stretches_the_paper_feed_only() {
        let s = Scale::for_paper(512, 1020);
        assert!((s.y / s.x - 1.02).abs() < 1e-9);
        assert_eq!(Scale::for_paper(512, 5000), Scale::for_paper(512, 1100), "bornée");
    }

    #[test]
    fn circle_has_the_right_size() {
        let s = tm88();
        let mut sheet = Sheet::new(s, 40.0);
        sheet.circle((20.0, 20.0), 15.0, 0.4, None);
        let img = sheet.into_image();
        let row = s.dy(20.0) as u32;
        let inked: Vec<u32> = (0..img.width()).filter(|&x| black(&img, x, row)).collect();
        let span = f64::from(inked.last().unwrap() - inked.first().unwrap()) / s.x;
        assert!((span - 30.0).abs() < 0.5, "diamètre mesuré : {span} mm");
    }
}
