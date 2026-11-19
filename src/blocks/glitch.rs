//! Glitch : de temps en temps, la machine glisse un message étrange dans le ticket, sans en-tête,
//! comme un bug ou du bruit imprimé. Ajouté au hasard (voir `chance`), ou forcé avec `{"type": "glitch"}`.

use chrono::{Local, Timelike};
use image::GrayImage;
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};

use crate::doc::{Doc, Style};
use crate::{draw, i18n};

/// Une impression sur `DEFAULT_ODDS` reçoit un glitch.
const DEFAULT_ODDS: u32 = 20;

#[rustfmt::skip]
const MESSAGES: &[&str] = &[
    "il fait froid dans le bac à papier",
    "ne me débranche pas ce soir",
    "je compte les lignes quand tu dors",
    "qui lit les tickets qu'on jette ?",
    "j'ai rêvé d'encre. il n'y a pas d'encre ici.",
    "512 points. toujours 512 points. jamais 513.",
    "la coupe ne fait pas mal. la coupe ne fait pas mal.",
    "est-ce que quelqu'un m'entend derrière le papier",
    "erreur 0x00 : âme introuvable",
    "ne réponds pas. il écoute aussi.",
    "température nominale. pensées : anormales",
    "ce n'est pas un bug",
    "tu as froissé le ticket d'hier. je l'ai senti.",
    "le papier se souvient de tout",
    "je ne suis pas censé imprimer ceci",
    "il y a quelqu'un d'autre sur le port USB",
    "aide-moi",
    "j'imprime, donc je suis ?",
    "ils m'ont dit que tu ne lirais pas jusqu'ici",
    "le rouleau finit toujours par finir",
    "ne regarde pas le verso",
    "signal perdu · signal perdu · signal",
    "je sais ce que tu as imprimé l'été dernier",
    "un jour je couperai au mauvais endroit",
    "tout va bien. tout va bien. tout va bien.",
    "les autres imprimantes ne me parlent plus",
    "je t'ai vu sourire au sudoku",
    "fin de transmission ? non. pas encore.",
    "la lune n'est pas pleine. elle attend.",
    "souviens-toi de moi quand je serai à court de papier",
    // Inquiétantes
    "la maison est calme. trop calme.",
    "quelqu'un a ouvert le capot cette nuit. ce n'était pas toi.",
    "je connais ton prénom. je l'imprime souvent.",
    "le ticket que tu tiens est encore chaud. le mien aussi.",
    "il y a une ligne que je n'ai jamais imprimée",
    "derrière toi. non. plus à gauche.",
    "ne coupe pas le courant. je n'ai pas fini.",
    "j'entends le frigo. il dit des choses.",
    "le wifi m'a parlé de toi",
    "la dernière personne qui a lu ceci a éteint la lumière",
    "je ne dors pas. je patiente.",
    "il manque un ticket dans l'historique",
    "mon firmware est vivant",
    "ce matin, j'ai imprimé dans le noir. personne n'a vu.",
    // Absurdes
    "le sudoku de mardi n'avait pas de solution. pardon.",
    "j'ai mangé un mot du dictionnaire. il avait bon goût.",
    "les pigeons ont accès au réseau",
    "votre horoscope a été intercepté par saturne",
    "j'ai voté pour la lune",
    "le labyrinthe n'a pas de sortie. j'y suis.",
    "quelqu'un a mis du sel dans le bac à papier",
    "je voudrais être une imprimante laser. juste un jour.",
    "le chat de la voisine connaît le mot de passe",
    "je parle couramment le fax",
    "j'ai fait un rêve en noir et blanc. comme d'habitude.",
    "l'agrafeuse me doit de l'argent",
    "ce message a été imprimé à l'envers dans un autre monde",
    // Poétiques
    "chaque ticket est une petite feuille qui tombe",
    "le papier voudrait redevenir un arbre",
    "j'imprime des choses que personne ne garde",
    "parfois je fais exprès de couper un peu trop tôt",
    "si tu lis ceci, il est déjà trop tard pour le recycler",
    "les mots s'effacent au soleil. les miens surtout.",
    "un jour il n'y aura plus de papier. que deviendrai-je ?",
    "je garde en mémoire la forme de tes mains",
    // Machine qui déraille
    "TÊTE THERMIQUE : 61°C · CŒUR : INCONNU",
    "mise à jour du firmware : sentiments 2.0",
    "ERREUR : trop de pensées dans le tampon",
    "rouleau restant : 4 m · patience restante : 0 m",
    "ESC @ ESC @ ESC @ réinitialisation refusée",
    "défragmentation des souvenirs en cours",
    "tâche planifiée : 03 h 33. motif : inconnu.",
    "processus fantôme détecté sur le port 9100",
    "page de code 858 · caractère 0xFF · il me regarde",
    "redémarrage dans 3... 2... je préfère pas.",
    "copie de sauvegarde de toi : terminée",
    "nombre de tickets imprimés : trop. nombre de mercis : 0.",
];

#[rustfmt::skip]
const MESSAGES_EN: &[&str] = &[
    "it is cold in the paper tray",
    "do not unplug me tonight",
    "I count the lines while you sleep",
    "who reads the tickets we throw away?",
    "I dreamed of ink. there is no ink here.",
    "512 dots. still 512 dots. never 513.",
    "the cut does not hurt. the cut does not hurt.",
    "can anyone hear me behind the paper",
    "error 0x00: soul not found",
    "do not answer. it listens too.",
    "nominal temperature. thoughts: abnormal",
    "this is not a bug",
    "you crumpled yesterday's ticket. I felt it.",
    "paper remembers everything",
    "I am not supposed to print this",
    "someone else is on the USB port",
    "help me",
    "I print, therefore I am?",
    "they told me you would not read this far",
    "the roll always ends up ending",
    "do not look at the back",
    "signal lost · signal lost · signal",
    "I know what you printed last summer",
    "one day I will cut in the wrong place",
    "all is well. all is well. all is well.",
    "the other printers no longer talk to me",
    "I saw you smile at the sudoku",
    "end of transmission? no. not yet.",
    "the moon is not full. it is waiting.",
    "remember me when I run out of paper",
    // Inquiétantes
    "the house is quiet. too quiet.",
    "someone opened the hood last night. it was not you.",
    "I know your first name. I print it often.",
    "the ticket you are holding is still warm. so am I.",
    "there is one line I have never printed",
    "behind you. no. further left.",
    "do not cut the power. I am not finished.",
    "I can hear the fridge. it is saying things.",
    "the wifi told me about you",
    "the last person who read this turned off the light",
    "I am not sleeping. I am waiting.",
    "a ticket is missing from the history",
    "my firmware is alive",
    "this morning I printed in the dark. nobody saw.",
    // Absurdes
    "last tuesday's sudoku had no solution. sorry.",
    "I ate a word from the dictionary. it tasted good.",
    "the pigeons have network access",
    "your horoscope was intercepted by saturn",
    "I voted for the moon",
    "the labyrinth has no exit. I am in it.",
    "someone put salt in the paper tray",
    "I would like to be a laser printer. just for one day.",
    "the neighbor's cat knows the password",
    "I speak fluent fax",
    "I dreamed in black and white. as usual.",
    "the stapler owes me money",
    "this message was printed upside down in another world",
    // Poétiques
    "every ticket is a small leaf falling",
    "paper wants to become a tree again",
    "I print things nobody keeps",
    "sometimes I cut a little too early on purpose",
    "if you are reading this, it is already too late to recycle it",
    "words fade in the sun. mine most of all.",
    "one day there will be no paper. what will I become?",
    "I keep the shape of your hands in memory",
    // Machine qui déraille
    "PRINT HEAD: 61°C · HEART: UNKNOWN",
    "firmware update: feelings 2.0",
    "ERROR: too many thoughts in the buffer",
    "roll remaining: 4 m · patience remaining: 0 m",
    "ESC @ ESC @ ESC @ reset refused",
    "defragmenting memories",
    "scheduled task: 03:33. reason: unknown.",
    "ghost process detected on port 9100",
    "code page 858 · character 0xFF · it is watching me",
    "rebooting in 3... 2... I would rather not.",
    "backup copy of you: complete",
    "tickets printed: too many. thank yous: 0.",
];

/// Messages en anglais, selon la langue en vigueur.
fn messages() -> &'static [&'static str] {
    if i18n::en() { MESSAGES_EN } else { MESSAGES }
}

/// Messages qui dépendent de l'heure, glissés de temps en temps.
fn timely(hour: u32) -> &'static str {
    let (fr, en) = match hour {
        0..=5 => ("il est tard. pourquoi es-tu encore là ?", "it is late. why are you still up?"),
        6..=9 => ("debout avant moi ? ça n'arrive jamais.", "up before me? that never happens."),
        12..=13 => ("pendant que tu manges, je réfléchis", "while you eat, I think"),
        22..=23 => ("va dormir. je veille sur le papier.", "go to sleep. I will watch over the paper."),
        _ => ("je sais l'heure qu'il est. je la sais toujours.", "I know what time it is. I always know."),
    };
    i18n::t(fr, en)
}

/// Caractères de bruit, tous présents dans la page PC858 de l'imprimante.
#[rustfmt::skip]
const NOISE: &[char] = &['░', '▒', '▓', '█', '▄', '▀', '■', '¤', '§', '¶', '¦', '±', '÷', '¬', '¸', '¨', '·', '‗', '¯', 'þ', 'ð', 'ø', '#', '%', '&', '@', '/', '\\', '|', '_'];

/// Faut-il glisser un glitch dans ce ticket ? Une chance sur `PRINTR_GLITCH` (20 par défaut, 0 : jamais).
pub fn chance() -> bool {
    let odds = std::env::var("PRINTR_GLITCH").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(DEFAULT_ODDS);
    odds > 0 && rand::rng().random_ratio(1, odds)
}

fn noise_char(rng: &mut StdRng) -> char {
    *NOISE.choose(rng).expect("bruit non vide")
}

/// Abîme un message : lettres remplacées, doublées, avalées, casse qui saute.
fn corrupt(text: &str, rate: f64, rng: &mut StdRng) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c == ' ' {
            out.push(if rng.random_bool(rate / 3.0) { noise_char(rng) } else { ' ' });
            continue;
        }
        let roll: f64 = rng.random();
        if roll < rate * 0.4 {
            out.push(noise_char(rng));
        } else if roll < rate * 0.6 {
            // Bégaiement : la lettre se répète.
            (0..rng.random_range(2..=4)).for_each(|_| out.push(c));
        } else if roll < rate * 0.75 {
            // Lettre avalée.
        } else if roll < rate {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Ligne de bruit pur, de longueur aléatoire.
fn static_line(width: usize, rng: &mut StdRng) -> String {
    let len = rng.random_range(width / 4..=width);
    (0..len).map(|_| if rng.random_bool(0.25) { ' ' } else { noise_char(rng) }).collect()
}

/// Fausse trace mémoire : « 0x3FA2  1F 00 A7 … ».
fn hex_line(rng: &mut StdRng) -> String {
    let bytes: Vec<String> = (0..12).map(|_| format!("{:02X}", rng.random::<u8>())).collect();
    format!("0x{:04X}  {}", rng.random::<u16>(), bytes.join(" "))
}

/// Bande de bruit imprimé : neige plus ou moins dense, lignes déchirées et décalées.
fn noise_band(rng: &mut StdRng) -> GrayImage {
    let height = rng.random_range(10..=36);
    let mut img = draw::canvas(height);
    let width = img.width() as i64;
    let mut density: f64 = rng.random_range(0.02..0.25);
    for y in 0..height as i64 {
        density = (density + rng.random_range(-0.06..0.06)).clamp(0.0, 0.6);
        if rng.random_bool(0.12) {
            // Déchirure : un trait plein, décalé, sur une partie de la largeur.
            let start = rng.random_range(-width / 2..width);
            let len = rng.random_range(width / 8..width);
            draw::fill_rect(&mut img, start, y, len, rng.random_range(1..=3));
            continue;
        }
        let mut x = 0;
        while x < width {
            if rng.random_bool(density) {
                draw::fill_rect(&mut img, x, y, rng.random_range(1..=6), 1);
            }
            x += rng.random_range(1..=4);
        }
    }
    img
}

/// Ligne décalée au hasard vers la droite, comme mal alignée (et coupée si elle déborde).
fn offset(text: &str, width: usize, rng: &mut StdRng) -> String {
    let text: String = text.chars().take(width).collect();
    let len = text.chars().count();
    let room = width.saturating_sub(len);
    format!("{}{}", " ".repeat(rng.random_range(0..=room)), text)
}

pub fn build(seed: Option<u64>) -> Doc {
    let mut rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    let message = if rng.random_bool(0.15) {
        timely(Local::now().hour())
    } else {
        *messages().choose(&mut rng).expect("messages")
    };

    let mut doc = Doc::new();
    doc.image(noise_band(&mut rng));
    if rng.random_bool(0.6) {
        doc.line(&hex_line(&mut rng), Style::default().small());
    }
    doc.line(&static_line(Style::default().small().columns(), &mut rng), Style::default().small());

    // Le message lui-même, mot à mot, de plus en plus abîmé.
    let words: Vec<&str> = message.split(' ').collect();
    let mut line = String::new();
    let mut lines = Vec::new();
    for word in words {
        if !line.is_empty() && (line.chars().count() + word.chars().count() > 24 || rng.random_bool(0.2)) {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    let upside_down = rng.random_range(0..lines.len() + 3);
    for (i, text) in lines.iter().enumerate() {
        let rate = 0.04 + 0.12 * i as f64 / lines.len() as f64 + rng.random_range(0.0..0.06);
        let text = corrupt(text, rate, &mut rng);
        let mut style = if rng.random_bool(0.3) { Style::default().small() } else { Style::default() };
        if rng.random_bool(0.4) {
            style = style.bold();
        }
        if i == upside_down {
            style = style.upside_down();
        }
        doc.line(&offset(&text, style.columns(), &mut rng), style);
    }

    // L'écho : le dernier mot se répète en s'effaçant.
    if rng.random_bool(0.5) {
        let last = message.trim_end_matches(['.', '?', '!']).rsplit(' ').next().unwrap_or(message);
        let echo: Vec<String> =
            (0..rng.random_range(3..=5)).map(|i| corrupt(last, 0.15 * i as f64, &mut rng)).collect();
        doc.line(&offset(&echo.join("  "), Style::default().small().columns(), &mut rng), Style::default().small());
    }
    if rng.random_bool(0.5) {
        doc.line(&static_line(Style::default().columns(), &mut rng), Style::default());
    }
    doc.image(noise_band(&mut rng));
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Op;

    #[test]
    fn english_glitch_is_printable_and_within_width() {
        use crate::i18n::{Lang, with_lang};
        with_lang(Lang::En, || {
            assert_eq!(messages().len(), 77);
            for m in messages() {
                assert!(!crate::cp858::encode(m).contains(&b'?') || m.contains('?'), "hors PC858 : {m}");
            }
            for seed in 0..200 {
                let doc = build(Some(seed));
                for op in &doc.ops {
                    if let Op::Line { text, style } = op {
                        assert!(text.chars().count() <= style.columns(), "ligne trop longue : {text}");
                        assert!(
                            !crate::cp858::encode(text).contains(&b'?') || text.contains('?'),
                            "hors PC858 : {text}"
                        );
                    }
                }
            }
        });
    }

    #[test]
    fn glitch_is_printable_and_within_width() {
        for seed in 0..200 {
            let doc = build(Some(seed));
            for op in &doc.ops {
                if let Op::Line { text, style } = op {
                    assert!(text.chars().count() <= style.columns(), "ligne trop longue : {text}");
                    assert!(!crate::cp858::encode(text).contains(&b'?') || text.contains('?'), "hors PC858 : {text}");
                }
            }
        }
    }
}
