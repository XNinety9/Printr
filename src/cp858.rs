//! Encodage du texte en page de code PC858 (CP850 + €), sélectionnée par `ESC t 19`.
//!
//! La table de la crate `escpos` contient des erreurs (`-` mappé sur 0xF0, `¿`/`®` décalés,
//! `Ò` absent) et envoie les caractères inconnus en UTF-8 brut : on encode donc nous-mêmes.

/// Caractères 0x80..=0xFF de la page CP858.
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

/// Remplacements pour les caractères courants absents de la page.
fn transliterate(c: char) -> Option<&'static str> {
    Some(match c {
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

fn encode_char(c: char, out: &mut Vec<u8>) {
    if c.is_ascii() {
        if c == '\t' {
            out.push(b' ');
        } else if !c.is_ascii_control() {
            out.push(c as u8);
        }
    } else if let Some(i) = HIGH.iter().position(|&h| h == c) {
        out.push(0x80 + i as u8);
    } else if let Some(s) = transliterate(c) {
        s.chars().for_each(|c| encode_char(c, out));
    } else {
        out.push(b'?');
    }
}

/// Encode une chaîne en PC858 ; les caractères non représentables deviennent `?`.
pub fn encode(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    text.chars().for_each(|c| encode_char(c, &mut out));
    out
}

/// Remplace les caractères non imprimables par leur équivalent, pour que la largeur
/// calculée au moment du retour à la ligne corresponde à ce qui sera imprimé.
pub fn normalize(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() || HIGH.contains(&c) {
            s.push(c);
        } else if let Some(t) = transliterate(c) {
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
    fn ascii_and_accents() {
        assert_eq!(encode("a-b"), b"a-b");
        assert_eq!(encode("é€°"), vec![0x82, 0xD5, 0xF8]);
        assert_eq!(encode("Ò¿®"), vec![0xE3, 0xA8, 0xA9]);
    }

    #[test]
    fn typography_is_transliterated() {
        assert_eq!(encode("l’œuf…"), b"l'oeuf...");
        assert_eq!(encode("😀"), b"?");
        assert_eq!(normalize("« l’été »"), "« l'été »");
    }
}
