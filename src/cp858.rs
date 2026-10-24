//! Encodage du texte en page de code PC858 (CP850 + €), sélectionnée par `ESC t 19`.
//!
//! Certaines imprimantes (clones chinois, p. ex. MUNBYN 047P) n'ont pas la page 19 : `PRINTR_CODEPAGE=850`
//! sélectionne alors PC850 (`ESC t 2`), identique à PC858 sauf le `€`, imprimé « EUR ».
//!
//! La table de la crate `escpos` contient des erreurs (`-` mappé sur 0xF0, `¿`/`®` décalés,
//! `Ò` absent) et envoie les caractères inconnus en UTF-8 brut : on encode donc nous-mêmes.

/// Caractères 0x80..=0xFF de la page CP858.
#[rustfmt::skip]
const HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', 'ø', '£', 'Ø', '×', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '®', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', 'Á', 'Â', 'À', '©', '╣', '║', '╗', '╝', '¢', '¥', '┐', //
    '└', '┴', '┬', '├', '─', '┼', 'ã', 'Ã', '╚', '╔', '╩', '╦', '╠', '═', '╬', '¤', //
    'ð', 'Ð', 'Ê', 'Ë', 'È', '€', 'Í', 'Î', 'Ï', '┘', '┌', '█', '▄', '¦', 'Ì', '▀', //
    'Ó', 'ß', 'Ô', 'Ò', 'õ', 'Õ', 'µ', 'þ', 'Þ', 'Ú', 'Û', 'Ù', 'ý', 'Ý', '¯', '´', //
    '\u{AD}', '±', '‗', '¾', '¶', '§', '÷', '¸', '°', '¨', '·', '¹', '³', '²', '■', '\u{A0}',
];

/// Page de code de l'imprimante : PC858 (défaut), ou PC850 avec `PRINTR_CODEPAGE=850` (ou `2`) pour
/// les imprimantes qui n'ont pas la page 19. PC850 n'a pas le `€` (0xD5 y est un `ı`).
pub fn is_pc850() -> bool {
    static PC850: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PC850.get_or_init(|| matches!(std::env::var("PRINTR_CODEPAGE").as_deref().map(str::trim), Ok("850" | "2")))
}

/// Remplacements pour les caractères courants absents de la page.
fn transliterate(c: char, pc850: bool) -> Option<&'static str> {
    Some(match c {
        '€' if pc850 => "EUR",
        '‘' | '’' | '‚' | '′' => "'",
        '“' | '”' | '„' | '″' => "\"",
        '–' | '—' | '−' | '‐' => "-",
        '…' => "...",
        'œ' => "oe",
        'Œ' => "OE",
        '\u{202F}' | '\u{2009}' | '\u{2007}' => " ",
        '•' => "·",
        '→' => "->",
        '←' => "<-",
        'Ÿ' => "Y",
        '\u{200B}' | '\u{FE0F}' => "",
        _ => return None,
    })
}

fn encode_char(c: char, pc850: bool, out: &mut Vec<u8>) {
    if c.is_ascii() {
        if c == '\t' {
            out.push(b' ');
        } else if !c.is_ascii_control() {
            out.push(c as u8);
        }
    } else if let Some(i) = HIGH.iter().position(|&h| h == c).filter(|_| !(pc850 && c == '€')) {
        out.push(0x80 + i as u8);
    } else if let Some(s) = transliterate(c, pc850) {
        s.chars().for_each(|c| encode_char(c, pc850, out));
    } else {
        out.push(b'?');
    }
}

/// Encode une chaîne en PC858 ; les caractères non représentables deviennent `?`.
pub fn encode(text: &str) -> Vec<u8> {
    encode_for(text, is_pc850())
}

fn encode_for(text: &str, pc850: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    text.chars().for_each(|c| encode_char(c, pc850, &mut out));
    out
}

/// Remplace les caractères non imprimables par leur équivalent, pour que la largeur
/// calculée au moment du retour à la ligne corresponde à ce qui sera imprimé.
pub fn normalize(text: &str) -> String {
    normalize_for(text, is_pc850())
}

fn normalize_for(text: &str, pc850: bool) -> String {
    let mut s = String::with_capacity(text.len());
    for c in text.chars() {
        // Aucun caractère de contrôle ne doit atteindre l'imprimante : ESC (0x1B) ou GS (0x1D)
        // y ouvriraient des commandes ESC/POS glissées dans un texte (API, MQTT, flux RSS…). Seul
        // le retour à la ligne reste, pour découper les paragraphes.
        if c == '\t' {
            s.push(' ');
        } else if c.is_control() && c != '\n' {
            continue;
        } else if c.is_ascii() || (HIGH.contains(&c) && !(pc850 && c == '€')) {
            s.push(c);
        } else if let Some(t) = transliterate(c, pc850) {
            s.push_str(t);
        } else {
            s.push('?');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_never_reach_the_printer() {
        assert_eq!(normalize("a\x1b@b\x1dVc\x07\r"), "a@bVc");
        assert_eq!(normalize("un\tdeux\ntrois\u{7f}\u{9b}"), "un deux\ntrois");
    }

    #[test]
    fn ascii_and_accents() {
        assert_eq!(encode("a-b"), b"a-b");
        assert_eq!(encode("é€°"), vec![0x82, 0xD5, 0xF8]);
        assert_eq!(encode("Ò¿®"), vec![0xE3, 0xA8, 0xA9]);
    }

    #[test]
    fn pc850_has_no_euro_sign() {
        // PC858 : « € » est l'octet 0xD5 ; PC850 : cet octet est un « ı », on écrit donc « EUR ».
        assert_eq!(encode_for("5€", false), vec![b'5', 0xD5]);
        assert_eq!(encode_for("5€", true), b"5EUR");
        assert_eq!(normalize_for("5€", false), "5€");
        assert_eq!(normalize_for("5€", true), "5EUR");
        // Les accents français sont les mêmes dans les deux pages.
        assert_eq!(encode_for("arrêté à bientôt", true), encode_for("arrêté à bientôt", false));
        assert!(!encode_for("é€\u{2019}", true).contains(&0xD5));
    }

    #[test]
    fn typography_is_transliterated() {
        assert_eq!(encode("l’œuf…"), b"l'oeuf...");
        assert_eq!(encode("😀"), b"?");
        assert_eq!(normalize("« l’été »"), "« l'été »");
    }
}
