//! Diagrammes d'accords pour guitare et basse, ou grilles vides à remplir.
//!
//! Les accords principaux de la guitare sont écrits à la main (doigtés ouverts habituels) ; les
//! autres, et tous ceux de la basse, viennent de formes mobiles posées sur la bonne case.

use anyhow::Result;
use rand::seq::IndexedRandom;
use serde::Deserialize;

use super::{Scale, Sheet, check_range};
use crate::doc::{Doc, Style};
use crate::draw::{BLACK, WHITE};
use crate::i18n;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Instrument {
    #[default]
    #[serde(alias = "guitare")]
    Guitar,
    #[serde(alias = "basse")]
    Bass,
}

impl Instrument {
    /// Cordes à vide, de la plus grave à la plus aiguë (numéros MIDI).
    fn tuning(self) -> &'static [u8] {
        match self {
            Instrument::Guitar => &[40, 45, 50, 55, 59, 64],
            Instrument::Bass => &[28, 33, 38, 43],
        }
    }

    fn string_names(self) -> &'static str {
        match self {
            Instrument::Guitar => "EADGBE",
            Instrument::Bass => "EADG",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Instrument::Guitar => i18n::t("guitare", "guitar"),
            Instrument::Bass => i18n::t("basse", "bass"),
        }
    }
}

/// Les accords proposés dans l'appli (`web/app.js`), où « au hasard » pioche.
const MAIN: &[&str] = &[
    "C", "Cm", "C7", "Cmaj7", "C#m", "D", "Dm", "D7", "Dm7", "Dsus2", "Dsus4", "E", "Em", "E7", "Em7", "Esus4", "F",
    "Fm", "Fmaj7", "F#m", "G", "Gm", "G7", "Gmaj7", "A", "Am", "A7", "Am7", "Amaj7", "Asus2", "Asus4", "Bb", "B", "Bm",
    "B7", "Bm7",
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Quality {
    Major,
    Minor,
    Seventh,
    Minor7,
    Major7,
    Sus2,
    Sus4,
}

impl Quality {
    const ALL: [(&'static str, Quality); 7] = [
        ("", Quality::Major),
        ("m", Quality::Minor),
        ("7", Quality::Seventh),
        ("m7", Quality::Minor7),
        ("maj7", Quality::Major7),
        ("sus2", Quality::Sus2),
        ("sus4", Quality::Sus4),
    ];

    /// Intervalles depuis la fondamentale, en demi-tons, avec le degré (0 : fondamentale, 2 :
    /// tierce…) pour écrire le nom des notes.
    fn tones(self) -> &'static [(u8, u8)] {
        match self {
            Quality::Major => &[(0, 0), (4, 2), (7, 4)],
            Quality::Minor => &[(0, 0), (3, 2), (7, 4)],
            Quality::Seventh => &[(0, 0), (4, 2), (7, 4), (10, 6)],
            Quality::Minor7 => &[(0, 0), (3, 2), (7, 4), (10, 6)],
            Quality::Major7 => &[(0, 0), (4, 2), (7, 4), (11, 6)],
            Quality::Sus2 => &[(0, 0), (2, 1), (7, 4)],
            Quality::Sus4 => &[(0, 0), (5, 3), (7, 4)],
        }
    }

    fn label(self) -> &'static str {
        match self {
            Quality::Major => i18n::t("majeur", "major"),
            Quality::Minor => i18n::t("mineur", "minor"),
            Quality::Seventh => i18n::t("septième", "seventh"),
            Quality::Minor7 => i18n::t("mineur septième", "minor seventh"),
            Quality::Major7 => i18n::t("septième majeure", "major seventh"),
            Quality::Sus2 => i18n::t("seconde suspendue", "suspended second"),
            Quality::Sus4 => i18n::t("quarte suspendue", "suspended fourth"),
        }
    }
}

/// Lettres des notes, avec leur hauteur depuis do.
const LETTERS: [(char, u8); 7] = [('C', 0), ('D', 2), ('E', 4), ('F', 5), ('G', 7), ('A', 9), ('B', 11)];
const SOLFEGE: [&str; 7] = ["Do", "Ré", "Mi", "Fa", "Sol", "La", "Si"];

#[derive(Clone, Copy, Debug, PartialEq)]
struct Chord {
    /// Indice de la lettre dans `LETTERS`.
    letter: usize,
    /// -1 bémol, 1 dièse.
    accidental: i8,
    quality: Quality,
}

impl Chord {
    /// « C », « F#m », « Bbmaj7 », « Dsus4 »… ou en solfège : « Do », « Fa#m », « Sibm ».
    fn parse(name: &str) -> Option<Chord> {
        let name = name.trim();
        let (letter, rest) = SOLFEGE
            .iter()
            .enumerate()
            .find_map(|(i, s)| {
                let plain = s.replace('é', "e");
                [s.to_string(), plain].into_iter().find_map(|p| strip_prefix_ci(name, &p).map(|r| (i, r)))
            })
            .or_else(|| {
                let c = name.chars().next()?.to_ascii_uppercase();
                let i = LETTERS.iter().position(|(l, _)| *l == c)?;
                Some((i, &name[1..]))
            })?;
        let (accidental, rest) = match rest.chars().next() {
            Some('#' | '♯') => (1, &rest[rest.chars().next()?.len_utf8()..]),
            Some('b' | '♭') => (-1, &rest[rest.chars().next()?.len_utf8()..]),
            _ => (0, rest),
        };
        let rest = match rest {
            "M7" | "Δ" | "Δ7" | "7M" => "maj7",
            "min" | "-" => "m",
            "min7" | "-7" => "m7",
            "maj" | "M" => "",
            "sus" => "sus4",
            r => r,
        };
        let quality = Quality::ALL.iter().find(|(s, _)| *s == rest)?.1;
        Some(Chord { letter, accidental, quality })
    }

    fn root(self) -> u8 {
        (i16::from(LETTERS[self.letter].1) + i16::from(self.accidental)).rem_euclid(12) as u8
    }

    #[cfg(test)]
    fn pitch_classes(self) -> Vec<u8> {
        self.quality.tones().iter().map(|(semi, _)| (self.root() + semi) % 12).collect()
    }

    /// Nom des notes, bien orthographiées : la tierce de mi majeur est sol#, pas lab.
    fn note_names(self) -> Vec<String> {
        self.quality
            .tones()
            .iter()
            .map(|&(semi, degree)| {
                let letter = (self.letter + usize::from(degree)) % 7;
                let natural = LETTERS[letter].1;
                let pitch = (self.root() + semi) % 12;
                let diff = (i16::from(pitch) - i16::from(natural) + 18).rem_euclid(12) - 6;
                let accidental = match diff {
                    -2 => "bb",
                    -1 => "b",
                    1 => "#",
                    2 => "##",
                    _ => "",
                };
                format!("{}{accidental}", LETTERS[letter].0)
            })
            .collect()
    }

    fn name(self) -> String {
        let suffix = Quality::ALL.iter().find(|(_, q)| *q == self.quality).map_or("", |(s, _)| s);
        format!("{}{}{suffix}", LETTERS[self.letter].0, accidental(self.accidental))
    }

    /// « Do majeur », « La mineur » ; en anglais, « C major ».
    fn spoken(self) -> String {
        if i18n::en() {
            let acc = match self.accidental {
                1 => " sharp",
                -1 => " flat",
                _ => "",
            };
            format!("{}{acc} {}", LETTERS[self.letter].0, self.quality.label())
        } else {
            let acc = match self.accidental {
                1 => " dièse",
                -1 => " bémol",
                _ => "",
            };
            format!("{}{acc} {}", SOLFEGE[self.letter], self.quality.label())
        }
    }
}

fn accidental(a: i8) -> &'static str {
    match a {
        1 => "#",
        -1 => "b",
        _ => "",
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &s[prefix.len()..])
}

/// Un doigté : la case de chaque corde (`None` : corde étouffée, 0 : à vide), le doigt qui la
/// tient (1 : index… 4 : auriculaire) et un barré éventuel (case, première et dernière corde).
#[derive(Clone, Debug, PartialEq)]
struct Shape {
    frets: Vec<Option<u8>>,
    fingers: Vec<u8>,
    barre: Option<(u8, usize, usize)>,
}

const X: i8 = -1;
const N: Option<i8> = None;

/// Accords ouverts de la guitare : cases (X étouffée) et doigts, de la corde de mi grave à la
/// corde de mi aiguë.
#[rustfmt::skip]
const GUITAR: &[(&str, [i8; 6], [u8; 6])] = &[
    ("C",     [X, 3, 2, 0, 1, 0], [0, 3, 2, 0, 1, 0]),
    ("C7",    [X, 3, 2, 3, 1, 0], [0, 3, 2, 4, 1, 0]),
    ("Cmaj7", [X, 3, 2, 0, 0, 0], [0, 3, 2, 0, 0, 0]),
    ("D",     [X, X, 0, 2, 3, 2], [0, 0, 0, 1, 3, 2]),
    ("Dm",    [X, X, 0, 2, 3, 1], [0, 0, 0, 2, 3, 1]),
    ("D7",    [X, X, 0, 2, 1, 2], [0, 0, 0, 2, 1, 3]),
    ("Dm7",   [X, X, 0, 2, 1, 1], [0, 0, 0, 2, 1, 1]),
    ("Dmaj7", [X, X, 0, 2, 2, 2], [0, 0, 0, 1, 2, 3]),
    ("Dsus2", [X, X, 0, 2, 3, 0], [0, 0, 0, 1, 3, 0]),
    ("Dsus4", [X, X, 0, 2, 3, 3], [0, 0, 0, 1, 3, 4]),
    ("E",     [0, 2, 2, 1, 0, 0], [0, 2, 3, 1, 0, 0]),
    ("Em",    [0, 2, 2, 0, 0, 0], [0, 2, 3, 0, 0, 0]),
    ("E7",    [0, 2, 0, 1, 0, 0], [0, 2, 0, 1, 0, 0]),
    ("Em7",   [0, 2, 0, 0, 0, 0], [0, 2, 0, 0, 0, 0]),
    ("Emaj7", [0, 2, 1, 1, 0, 0], [0, 3, 1, 2, 0, 0]),
    ("Esus4", [0, 2, 2, 2, 0, 0], [0, 2, 3, 4, 0, 0]),
    ("Fmaj7", [X, X, 3, 2, 1, 0], [0, 0, 3, 2, 1, 0]),
    ("G",     [3, 2, 0, 0, 0, 3], [2, 1, 0, 0, 0, 3]),
    ("G7",    [3, 2, 0, 0, 0, 1], [3, 2, 0, 0, 0, 1]),
    ("Gmaj7", [3, 2, 0, 0, 0, 2], [3, 2, 0, 0, 0, 1]),
    ("A",     [X, 0, 2, 2, 2, 0], [0, 0, 1, 2, 3, 0]),
    ("Am",    [X, 0, 2, 2, 1, 0], [0, 0, 2, 3, 1, 0]),
    ("A7",    [X, 0, 2, 0, 2, 0], [0, 0, 2, 0, 3, 0]),
    ("Am7",   [X, 0, 2, 0, 1, 0], [0, 0, 2, 0, 1, 0]),
    ("Amaj7", [X, 0, 2, 1, 2, 0], [0, 0, 2, 1, 3, 0]),
    ("Asus2", [X, 0, 2, 2, 0, 0], [0, 0, 1, 2, 0, 0]),
    ("Asus4", [X, 0, 2, 2, 3, 0], [0, 0, 1, 2, 3, 0]),
    ("B7",    [X, 2, 1, 2, 0, 2], [0, 2, 1, 3, 0, 4]),
    ("Bm7",   [X, 2, 0, 2, 0, 2], [0, 1, 0, 2, 0, 3]),
];

/// Forme mobile : décalage de chaque corde depuis la case de la fondamentale (`None` :
/// étouffée), doigts, et barré de l'index sur les cordes indiquées.
type Movable = (&'static [Option<i8>], &'static [u8], Option<(usize, usize)>);

/// Barrés de guitare, fondamentale sur la corde de mi grave (forme de mi).
fn guitar_e_shape(q: Quality) -> Movable {
    match q {
        Quality::Major => (&[Some(0), Some(2), Some(2), Some(1), Some(0), Some(0)], &[1, 3, 4, 2, 1, 1], Some((0, 5))),
        Quality::Minor => (&[Some(0), Some(2), Some(2), Some(0), Some(0), Some(0)], &[1, 3, 4, 1, 1, 1], Some((0, 5))),
        Quality::Seventh => {
            (&[Some(0), Some(2), Some(0), Some(1), Some(0), Some(0)], &[1, 3, 1, 2, 1, 1], Some((0, 5)))
        }
        Quality::Minor7 => (&[Some(0), Some(2), Some(0), Some(0), Some(0), Some(0)], &[1, 3, 1, 1, 1, 1], Some((0, 5))),
        Quality::Major7 => (&[Some(0), N, Some(1), Some(1), Some(0), N], &[1, 0, 3, 4, 2, 0], None),
        Quality::Sus4 => (&[Some(0), Some(2), Some(2), Some(2), Some(0), Some(0)], &[1, 2, 3, 4, 1, 1], Some((0, 5))),
        // Pas de forme de mi commode : on passe à la forme de la.
        Quality::Sus2 => (&[], &[], None),
    }
}

/// Barrés de guitare, fondamentale sur la corde de la (forme de la).
fn guitar_a_shape(q: Quality) -> Movable {
    match q {
        Quality::Major => (&[N, Some(0), Some(2), Some(2), Some(2), Some(0)], &[0, 1, 2, 3, 4, 1], Some((1, 5))),
        Quality::Minor => (&[N, Some(0), Some(2), Some(2), Some(1), Some(0)], &[0, 1, 3, 4, 2, 1], Some((1, 5))),
        Quality::Seventh => (&[N, Some(0), Some(2), Some(0), Some(2), Some(0)], &[0, 1, 3, 1, 4, 1], Some((1, 5))),
        Quality::Minor7 => (&[N, Some(0), Some(2), Some(0), Some(1), Some(0)], &[0, 1, 3, 1, 2, 1], Some((1, 5))),
        Quality::Major7 => (&[N, Some(0), Some(2), Some(1), Some(2), Some(0)], &[0, 1, 3, 2, 4, 1], Some((1, 5))),
        Quality::Sus2 => (&[N, Some(0), Some(2), Some(2), Some(0), Some(0)], &[0, 1, 3, 4, 1, 1], Some((1, 5))),
        Quality::Sus4 => (&[N, Some(0), Some(2), Some(2), Some(3), Some(0)], &[0, 1, 2, 3, 4, 1], Some((1, 5))),
    }
}

/// Basse, fondamentale sur la corde de mi : fondamentale, quinte, octave, puis la tierce (ou la
/// septième) au-dessus.
fn bass_e_shape(q: Quality) -> Movable {
    match q {
        Quality::Major => (&[Some(0), Some(2), Some(2), Some(1)], &[], None),
        Quality::Minor => (&[Some(0), Some(2), Some(2), Some(0)], &[], None),
        Quality::Seventh => (&[Some(0), Some(2), Some(0), Some(1)], &[], None),
        Quality::Minor7 => (&[Some(0), Some(2), Some(0), Some(0)], &[], None),
        Quality::Major7 => (&[Some(0), Some(2), Some(1), Some(1)], &[], None),
        Quality::Sus2 => (&[Some(0), Some(2), Some(2), Some(-1)], &[], None),
        Quality::Sus4 => (&[Some(0), Some(2), Some(2), Some(2)], &[], None),
    }
}

/// Basse, fondamentale sur la corde de la : la triade serrée (do : x 3 2 0).
fn bass_a_shape(q: Quality) -> Movable {
    match q {
        Quality::Major => (&[N, Some(0), Some(-1), Some(-3)], &[], None),
        Quality::Minor => (&[N, Some(0), Some(-2), Some(-3)], &[], None),
        Quality::Seventh => (&[N, Some(0), Some(-1), Some(0)], &[], None),
        Quality::Minor7 => (&[N, Some(0), Some(-2), Some(0)], &[], None),
        Quality::Major7 => (&[N, Some(0), Some(-1), Some(1)], &[], None),
        Quality::Sus2 => (&[N, Some(0), Some(-3), Some(-3)], &[], None),
        Quality::Sus4 => (&[N, Some(0), Some(0), Some(-3)], &[], None),
    }
}

/// Pose une forme mobile sur la case `root` ; `None` si une note tombe avant le sillet.
fn place((offsets, fingers, barre): Movable, root: u8) -> Option<Shape> {
    if offsets.is_empty() {
        return None;
    }
    let frets = offsets
        .iter()
        .map(|o| match o {
            None => Some(None),
            Some(o) => u8::try_from(i16::from(root) + i16::from(*o)).ok().map(Some),
        })
        .collect::<Option<Vec<_>>>()?;
    // Les doigts de la forme, ou un doigt par case depuis la plus basse ; au sillet, la forme
    // devient un accord ouvert et ses doigts ne valent plus.
    let lowest = frets.iter().flatten().copied().filter(|&f| f > 0).min().unwrap_or(1);
    let fingers = if fingers.is_empty() || root == 0 {
        frets.iter().map(|f| f.filter(|&f| f > 0).map_or(0, |f| (f - lowest + 1).min(4))).collect()
    } else {
        fingers.to_vec()
    };
    let barre = barre.filter(|_| root > 0).map(|(a, b)| (root, a, b));
    Some(Shape { frets, fingers, barre })
}

fn shape(instrument: Instrument, chord: Chord) -> Shape {
    let tuning = instrument.tuning();
    let fret_of = |string: usize| (chord.root() + 12 - tuning[string] % 12) % 12;
    let (e, a) = (fret_of(0), fret_of(1));
    if instrument == Instrument::Guitar
        && let Some((_, frets, fingers)) = GUITAR.iter().find(|(n, ..)| *n == chord.name())
    {
        return Shape {
            frets: frets.iter().map(|&f| u8::try_from(f).ok()).collect(),
            fingers: fingers.to_vec(),
            barre: None,
        };
    }
    let candidates = match instrument {
        Instrument::Guitar => vec![place(guitar_e_shape(chord.quality), e), place(guitar_a_shape(chord.quality), a)],
        Instrument::Bass => vec![
            place(bass_e_shape(chord.quality), e),
            place(bass_a_shape(chord.quality), a),
            place(bass_e_shape(chord.quality), e + 12),
            place(bass_a_shape(chord.quality), a + 12),
        ],
    };
    // La position la plus basse sur le manche ; à égalité, la fondamentale sur la corde grave.
    candidates
        .into_iter()
        .flatten()
        .min_by_key(|s| s.frets.iter().flatten().max().copied().unwrap_or(0))
        .expect("toujours une forme jouable")
}

/// Cases visibles sur un diagramme.
const FRETS: u8 = 5;

/// Dimensions d'une grille, en mm.
struct Grid {
    gap: f64,
    fret: f64,
    line: f64,
}

/// Dessine la grille (cordes, cases, sillet) dont le coin haut gauche est (x0, top) ; `start`
/// est la première case montrée (1 : le sillet est dessiné).
fn draw_grid(sheet: &mut Sheet, strings: usize, (x0, top): (f64, f64), g: &Grid, start: u8) {
    let width = g.gap * (strings - 1) as f64;
    let bottom = top + f64::from(FRETS) * g.fret;
    for i in 0..strings {
        let x = x0 + i as f64 * g.gap;
        sheet.line((x, top), (x, bottom), g.line, None);
    }
    for k in 0..=FRETS {
        let y = top + f64::from(k) * g.fret;
        sheet.line((x0, y), (x0 + width, y), g.line, None);
    }
    if start == 1 {
        let nut = (g.fret * 0.13).max(0.8);
        sheet.line((x0, top - nut / 2.0), (x0 + width, top - nut / 2.0), nut, None);
    }
}

fn string_names(sheet: &mut Sheet, instrument: Instrument, x0: f64, y: f64, gap: f64, scale: i64) {
    for (i, c) in instrument.string_names().chars().enumerate() {
        sheet.label(&c.to_string(), (x0 + i as f64 * gap, y), scale, BLACK);
    }
}

fn diagram(instrument: Instrument, shape: &Shape, s: Scale) -> image::GrayImage {
    let strings = shape.frets.len();
    let g = match instrument {
        Instrument::Guitar => Grid { gap: 8.0, fret: 9.0, line: 0.3 },
        Instrument::Bass => Grid { gap: 10.0, fret: 9.0, line: 0.4 },
    };
    let fretted = shape.frets.iter().flatten().copied().filter(|&f| f > 0);
    let highest = fretted.clone().max().unwrap_or(0);
    let start = if highest <= FRETS { 1 } else { fretted.min().unwrap_or(1) };
    let width = g.gap * (strings - 1) as f64;
    let x0 = (s.width_mm() - width) / 2.0;
    let top = 7.0;
    let bottom = top + f64::from(FRETS) * g.fret;
    let mut sheet = Sheet::new(s, bottom + 7.0);
    draw_grid(&mut sheet, strings, (x0, top), &g, start);
    let x = |i: usize| x0 + i as f64 * g.gap;
    let y = |fret: u8| top + (f64::from(fret) - f64::from(start) + 0.5) * g.fret;
    let r = g.gap.min(g.fret) * 0.36;
    // La case de départ, à gauche, hors des points posés sur la corde grave.
    if start > 1 {
        sheet.label_right(&start.to_string(), (x0 - r - 1.2, y(start)), s.digits());
    }
    // Au-dessus du sillet : o pour une corde à vide, × pour une corde étouffée.
    let mark = top - 3.5;
    for (i, fret) in shape.frets.iter().enumerate() {
        match fret {
            None => {
                let d = 1.2;
                sheet.line((x(i) - d, mark - d), (x(i) + d, mark + d), 0.35, None);
                sheet.line((x(i) - d, mark + d), (x(i) + d, mark - d), 0.35, None);
            }
            Some(0) => sheet.circle((x(i), mark), 1.3, 0.35, None),
            Some(_) => {}
        }
    }
    if let Some((fret, a, b)) = shape.barre {
        sheet.line((x(a), y(fret)), (x(b), y(fret)), 2.0 * r, None);
    }
    for (i, (fret, finger)) in shape.frets.iter().zip(&shape.fingers).enumerate() {
        let Some(fret) = fret.filter(|&f| f > 0) else { continue };
        sheet.disc((x(i), y(fret)), r, BLACK);
        if *finger > 0 {
            sheet.label(&finger.to_string(), (x(i), y(fret)), s.digits(), WHITE);
        }
    }
    string_names(&mut sheet, instrument, x0, bottom + 3.5, g.gap, s.digits());
    sheet.into_image()
}

/// `count` grilles vides, deux par rangée, avec la place d'écrire le nom de l'accord au-dessus.
fn blank(instrument: Instrument, count: u8, s: Scale) -> image::GrayImage {
    let strings = instrument.tuning().len();
    let g = match instrument {
        Instrument::Guitar => Grid { gap: 5.0, fret: 6.0, line: 0.25 },
        Instrument::Bass => Grid { gap: 6.5, fret: 6.0, line: 0.3 },
    };
    let width = g.gap * (strings - 1) as f64;
    let row = 10.0 + f64::from(FRETS) * g.fret + 6.0;
    let rows = count.div_ceil(2);
    let mut sheet = Sheet::new(s, f64::from(rows) * row);
    for n in 0..count {
        let (r, c) = (n / 2, n % 2);
        // Une grille seule sur sa rangée se met au milieu.
        let center = if n + 1 == count && c == 0 {
            0.5
        } else if c == 0 {
            0.25
        } else {
            0.75
        };
        let x0 = s.width_mm() * center - width / 2.0;
        let top = f64::from(r) * row + 10.0;
        draw_grid(&mut sheet, strings, (x0, top), &g, 1);
        string_names(&mut sheet, instrument, x0, top + f64::from(FRETS) * g.fret + 3.0, g.gap, s.small_digits());
    }
    sheet.into_image()
}

/// « random » ou « au hasard » : un des accords principaux, tiré à chaque impression.
pub fn is_random(name: &str) -> bool {
    let n = name.trim().to_lowercase().replace(['_', '-'], " ");
    ["random", "hasard", "au hasard"].contains(&n.as_str())
}

pub fn is_blank(name: &str) -> bool {
    let n = name.trim().to_lowercase().replace(['_', '-'], " ");
    ["blank", "vide", "grille vide", "grilles vides", "empty", "blank grid"].contains(&n.as_str())
}

/// `chord` : « C », « Am », « G7 »… ou « blank » pour `count` grilles vides.
pub fn build(instrument: Instrument, chord: &str, count: u8, s: Scale) -> Result<Doc> {
    let mut doc = Doc::new();
    if is_blank(chord) {
        check_range(i18n::t("nombre de grilles", "number of grids"), f64::from(count), 1.0, 12.0, "")?;
        doc.image(blank(instrument, count, s));
        return Ok(doc);
    }
    let chord = if is_random(chord) { MAIN.choose(&mut rand::rng()).expect("au moins un accord") } else { chord };
    let parsed = Chord::parse(chord).ok_or_else(|| {
        anyhow::anyhow!(tr!(
            "accord inconnu : « {chord} » (par exemple C, Am, G7, Cmaj7, Dsus4, F#m, ou « blank » pour des grilles vides)",
            "unknown chord: “{chord}” (for example C, Am, G7, Cmaj7, Dsus4, F#m, or “blank” for empty grids)"
        ))
    })?;
    let shape = shape(instrument, parsed);
    doc.text(&parsed.name(), Style::default().bold().size(3).center());
    doc.text(&format!("{} · {}", parsed.spoken(), instrument.label()), Style::default().small().center());
    doc.image(diagram(instrument, &shape, s));
    let notes = parsed.note_names().join(" ");
    doc.text(&tr!("Notes : {notes}", "Notes: {notes}"), Style::default().small().center());
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::super::tests::tm88;
    use super::*;

    /// Les notes jouées, de la plus grave à la plus aiguë.
    fn sounding(instrument: Instrument, shape: &Shape) -> Vec<u8> {
        shape.frets.iter().zip(instrument.tuning()).filter_map(|(f, t)| f.map(|f| t + f)).collect()
    }

    fn check(instrument: Instrument, name: &str) {
        let chord = Chord::parse(name).unwrap_or_else(|| panic!("{name} illisible"));
        let shape = shape(instrument, chord);
        let notes = sounding(instrument, &shape);
        let pcs: Vec<u8> = notes.iter().map(|n| n % 12).collect();
        let tones = chord.pitch_classes();
        assert!(
            pcs.iter().all(|p| tones.contains(p)),
            "{name} ({instrument:?}) : note étrangère dans {:?}",
            shape.frets
        );
        assert_eq!(pcs[0], chord.root(), "{name} ({instrument:?}) : la basse n'est pas la fondamentale");
        // Toutes les notes de l'accord, sauf la quinte que l'on peut omettre.
        for (t, (semi, _)) in tones.iter().zip(chord.quality.tones()) {
            assert!(
                *semi == 7 || pcs.contains(t),
                "{name} ({instrument:?}) : il manque une note dans {:?}",
                shape.frets
            );
        }
        let fretted: Vec<u8> = shape.frets.iter().flatten().copied().filter(|&f| f > 0).collect();
        if let (Some(lo), Some(hi)) = (fretted.iter().min(), fretted.iter().max()) {
            assert!(hi - lo <= 3, "{name} ({instrument:?}) : écart trop grand {:?}", shape.frets);
        }
        assert_eq!(shape.fingers.len(), shape.frets.len());
        for (f, finger) in shape.frets.iter().zip(&shape.fingers) {
            assert_eq!(f.is_some_and(|f| f > 0), *finger > 0, "{name} ({instrument:?}) : doigts {:?}", shape.fingers);
        }
    }

    #[test]
    fn every_chord_is_right_on_both_instruments() {
        let roots = ["C", "C#", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
        for instrument in [Instrument::Guitar, Instrument::Bass] {
            for name in MAIN {
                check(instrument, name);
            }
            for root in roots {
                for (suffix, _) in Quality::ALL {
                    check(instrument, &format!("{root}{suffix}"));
                }
            }
        }
    }

    #[test]
    fn usual_shapes() {
        let frets = |i, n| shape(i, Chord::parse(n).unwrap()).frets;
        let x = None;
        assert_eq!(frets(Instrument::Guitar, "C"), [x, Some(3), Some(2), Some(0), Some(1), Some(0)]);
        assert_eq!(frets(Instrument::Guitar, "F"), [Some(1), Some(3), Some(3), Some(2), Some(1), Some(1)]);
        assert_eq!(frets(Instrument::Guitar, "Bm"), [x, Some(2), Some(4), Some(4), Some(3), Some(2)]);
        assert_eq!(frets(Instrument::Bass, "C"), [x, Some(3), Some(2), Some(0)]);
        assert_eq!(frets(Instrument::Bass, "G"), [Some(3), Some(5), Some(5), Some(4)]);
    }

    #[test]
    fn names_and_spelling() {
        let c = |n| Chord::parse(n).unwrap();
        assert_eq!(c("E").note_names(), ["E", "G#", "B"]);
        assert_eq!(c("Bbm7").note_names(), ["Bb", "Db", "F", "Ab"]);
        assert_eq!(c("Lam"), c("Am"));
        assert_eq!(c("Ré7"), c("D7"));
        assert_eq!(c("solmaj7"), c("Gmaj7"));
        assert_eq!(c("Fa#m").name(), "F#m");
        assert!(Chord::parse("H").is_none() && Chord::parse("Cdim").is_none());
    }

    #[test]
    fn chord_builds() {
        let s = tm88();
        for instrument in [Instrument::Guitar, Instrument::Bass] {
            assert!(build(instrument, "Am", 6, s).is_ok());
            assert!(build(instrument, "B", 6, s).is_ok());
            assert!(build(instrument, "blank", 5, s).is_ok());
            assert!(build(instrument, "Grille vide", 13, s).is_err());
            assert!(build(instrument, "au_hasard", 1, s).is_ok());
        }
        assert!(build(Instrument::Guitar, "Xyz", 1, s).is_err());
        // Assez de place pour les grilles de la basse aussi, sur le papier le plus étroit.
        assert!(build(Instrument::Bass, "B", 1, Scale::for_paper(576, 1000)).is_ok());
    }
}
