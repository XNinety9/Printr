//! Fortune : un petit papier façon biscuit chinois (maxime, proverbe, prédiction farfelue), tiré au
//! hasard dans une liste locale, ou donné par la commande `fortune` du système (paquet `fortune-mod`).
//! En option, c'est une vache qui le dit, comme `cowsay`.

use anyhow::{Context, Result, bail};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::Deserialize;

use crate::doc::{Align, Doc, Style};
use crate::i18n;

/// D'où vient la phrase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// La liste de Printr, en français ou en anglais.
    #[default]
    #[serde(alias = "printr")]
    Builtin,
    /// La commande `fortune` installée sur la machine.
    #[serde(alias = "systeme")]
    System,
}

#[rustfmt::skip]
const FORTUNES: &[&str] = &[
    "Une chaussette perdue reviendra bientôt dans votre vie.",
    "Le café de demain matin sera exceptionnel.",
    "Vous retrouverez ce que vous cherchez dans la dernière poche que vous fouillerez.",
    "Quelqu'un pense à vous en ce moment. Probablement pour vous demander quelque chose.",
    "Un repas inattendu vous attend. Vérifiez le fond du frigo.",
    "Aujourd'hui, la file d'attente la plus lente sera celle que vous choisirez.",
    "Une bonne nouvelle arrivera par la porte d'entrée. Ou par la fenêtre, si elle est ouverte.",
    "Vous allez rire aujourd'hui. Peut-être de vous-même, mais c'est un bon début.",
    "Les étoiles vous sont favorables. Le lave-vaisselle, un peu moins.",
    "La patience est une vertu. Le dessert aussi.",
    "Ce n'est pas la destination qui compte, c'est d'avoir pensé aux clés.",
    "Un petit pas pour vous, une grande victoire contre le canapé.",
    "Votre plante verte vous remercie de penser à l'arroser. Elle insiste.",
    "Vous aurez une idée de génie sous la douche. Ayez un crayon étanche.",
    "La chance sourit à ceux qui rangent leur chambre.",
    "Le chat de la maison sait quelque chose que vous ignorez.",
    "Une rencontre importante vous attend : celle avec votre oreiller, ce soir.",
    "Aujourd'hui, dites merci deux fois de plus que d'habitude.",
    "Quelqu'un a besoin de votre sourire aujourd'hui. Il est gratuit, profitez-en.",
    "Il n'est jamais trop tard pour un deuxième petit-déjeuner.",
    "Méfiez-vous des chaussettes dépareillées : elles complotent.",
    "Le meilleur moment pour planter un arbre était il y a vingt ans. Le deuxième, c'est aujourd'hui.",
    "Qui va doucement va sûrement, surtout le lundi matin.",
    "Les petits ruisseaux font les grandes rivières.",
    "Petit à petit, l'oiseau fait son nid.",
    "Après la pluie, le beau temps.",
    "Il n'y a pas de sot métier, il n'y a que de sottes gens.",
    "Un tiens vaut mieux que deux tu l'auras.",
    "Qui sème le vent récolte la tempête.",
    "La nuit porte conseil.",
    "Une erreur de plus, c'est une leçon de plus.",
    "Votre prochaine bonne action sera remarquée. Par au moins un pigeon.",
    "Vous gagnerez une partie de jeu de société cette semaine. Ou presque.",
    "Un livre oublié sur une étagère n'attend que vous.",
    "La télécommande est sous le coussin de gauche.",
    "Aujourd'hui est un excellent jour pour ne rien remettre à demain. Sauf la vaisselle.",
    "Le bonheur est dans le pré. Vérifiez quand même la météo.",
    "Quelqu'un de la maison vous prépare une surprise. Faites semblant de ne rien savoir.",
    "Vous apprendrez quelque chose de nouveau avant ce soir.",
    "Un compliment sincère vaut mieux que dix cadeaux.",
    "Le temps passé à rire est du temps passé avec les dieux.",
    "Votre parapluie vous manquera aujourd'hui précisément parce que vous l'avez laissé.",
    "La réponse à votre question est : oui, mais pas tout de suite.",
    "Un vieux copain va bientôt donner des nouvelles.",
    "Aujourd'hui, la fortune vous sourit. Souriez-lui en retour.",
];

#[rustfmt::skip]
const FORTUNES_EN: &[&str] = &[
    "A lost sock will soon come back into your life.",
    "Tomorrow's morning coffee will be exceptional.",
    "You will find what you are looking for in the last pocket you check.",
    "Someone is thinking of you right now. Probably to ask you for something.",
    "An unexpected meal awaits you. Check the back of the fridge.",
    "Today, the slowest line will be the one you pick.",
    "Good news will come through the front door. Or the window, if it is open.",
    "You will laugh today. Maybe at yourself, but that is a good start.",
    "The stars are on your side. The dishwasher, less so.",
    "Patience is a virtue. So is dessert.",
    "It is not the destination that matters, it is remembering the keys.",
    "One small step for you, one giant victory over the couch.",
    "Your houseplant thanks you for thinking of watering it. It insists.",
    "You will have a brilliant idea in the shower. Keep a waterproof pencil.",
    "Fortune favours those who tidy their room.",
    "The cat knows something you don't.",
    "An important meeting awaits you: with your pillow, tonight.",
    "Today, say thank you twice more than usual.",
    "Someone needs your smile today. It is free, go for it.",
    "It is never too late for a second breakfast.",
    "Beware of odd socks: they are plotting something.",
    "The best time to plant a tree was twenty years ago. The second best time is now.",
    "Slow and steady wins the race, especially on Monday mornings.",
    "Every cloud has a silver lining.",
    "Rome wasn't built in a day.",
    "A journey of a thousand miles begins with a single step.",
    "The early bird catches the worm. The second mouse gets the cheese.",
    "Don't count your chickens before they hatch.",
    "Where there's a will, there's a way.",
    "Sleep on it: the night brings counsel.",
    "One more mistake is one more lesson.",
    "Your next good deed will be noticed. By at least one pigeon.",
    "You will win a board game this week. Or almost.",
    "A forgotten book on a shelf is waiting for you.",
    "The remote is under the left cushion.",
    "Today is a great day not to put anything off. Except the dishes.",
    "Happiness is just around the corner. Check the weather anyway.",
    "Someone at home is planning a surprise for you. Act like you know nothing.",
    "You will learn something new before tonight.",
    "One sincere compliment is worth ten presents.",
    "Time spent laughing is time spent with the gods.",
    "You will miss your umbrella today, precisely because you left it at home.",
    "The answer to your question is: yes, but not right away.",
    "An old friend will soon get in touch.",
    "Fortune smiles on you today. Smile back.",
];

/// La vache de `cowsay`, sous la bulle.
const COW: &[&str] = &[
    "        \\   ^__^",
    "         \\  (oo)\\_______",
    "            (__)\\       )\\/\\",
    "                ||----w |",
    "                ||     ||",
];

/// Largeur du texte dans la bulle : la bulle tient sur les 42 colonnes de la police A.
const BUBBLE: usize = 36;

/// Le texte, en lignes d'au plus `width` caractères (coupées entre les mots si possible).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_owned();
        // Un mot plus long que la ligne : coupé net.
        while word.chars().count() > width {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            lines.push(head);
        }
        let len = line.chars().count();
        if len > 0 && len + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// La bulle de `cowsay` autour du texte, puis la vache.
fn cow_says(text: &str) -> Vec<String> {
    let lines = wrap(text, BUBBLE);
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let pad = |l: &str| format!("{l}{}", " ".repeat(width - l.chars().count()));
    let mut out = vec![format!(" {}", "_".repeat(width + 2))];
    match lines.as_slice() {
        [only] => out.push(format!("< {} >", pad(only))),
        _ => {
            for (i, l) in lines.iter().enumerate() {
                let (left, right) = match i {
                    0 => ('/', '\\'),
                    i if i == lines.len() - 1 => ('\\', '/'),
                    _ => ('|', '|'),
                };
                out.push(format!("{left} {} {right}", pad(l)));
            }
        }
    }
    out.push(format!(" {}", "-".repeat(width + 2)));
    out.extend(COW.iter().map(|l| l.to_string()));
    out
}

/// Où chercher la commande `fortune` : seulement les emplacements des paquets (Debian et Raspberry Pi
/// OS l'installent dans /usr/games, qui n'est pas dans le PATH du service). Jamais le PATH : un faux
/// `fortune` placé dans un de ses dossiers serait lancé à la place.
const FORTUNE_PATHS: &[&str] = &["/usr/games/fortune", "/usr/bin/fortune", "/usr/local/bin/fortune"];
/// Au-delà, la commande est arrêtée.
const FORTUNE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
/// Longueur maximale gardée de sa sortie.
const FORTUNE_MAX: usize = 400;

/// La phrase de la commande `fortune` (version courte, `-s`), sur une seule ligne. Argument fixe, sans
/// shell : rien de ce que saisit l'utilisateur n'atteint la commande.
fn system_fortune() -> Result<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let program = FORTUNE_PATHS.iter().find(|p| std::path::Path::new(p).is_file()).with_context(|| {
        i18n::t(
            "commande fortune introuvable (paquet fortune-mod : sudo apt install fortune-mod)",
            "fortune command not found (fortune-mod package: sudo apt install fortune-mod)",
        )
    })?;
    let mut child = Command::new(program)
        .arg("-s")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_clear()
        .spawn()
        .with_context(|| tr!("impossible de lancer {program}", "cannot run {program}"))?;
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > FORTUNE_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{}", i18n::t("la commande fortune ne répond pas", "the fortune command does not answer"));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.by_ref().take(16 * 1024).read_to_string(&mut out);
    }
    let text: String = out.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(FORTUNE_MAX).collect();
    if !status.success() || text.is_empty() {
        bail!("{}", i18n::t("la commande fortune n'a rien donné", "the fortune command gave nothing"));
    }
    Ok(text)
}

pub fn build(source: Source, cow: bool, seed: Option<u64>) -> Result<Doc> {
    let (text, number) = match source {
        Source::Builtin => {
            let list = if i18n::en() { FORTUNES_EN } else { FORTUNES };
            let seed = seed.unwrap_or_else(|| rand::rng().random_range(1..100_000));
            let index = StdRng::seed_from_u64(seed).random_range(0..list.len());
            (list[index].to_owned(), Some(seed))
        }
        Source::System => (system_fortune()?, None),
    };
    let mut doc = Doc::new();
    if cow {
        for line in cow_says(&text) {
            doc.line(&line, Style::default());
        }
    } else {
        doc.text(&text, Style::default().bold().center());
    }
    if let Some(n) = number {
        doc.text(&tr!("n° {n}", "No. {n}"), Style::default().small().align(Align::Right));
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fortune_fits_and_lists_match() {
        assert_eq!(FORTUNES.len(), FORTUNES_EN.len());
        for text in FORTUNES.iter().chain(FORTUNES_EN) {
            assert!(cow_says(text).iter().all(|l| l.chars().count() <= 42), "trop large : {text}");
        }
    }

    #[test]
    fn the_cow_speaks_in_a_bubble() {
        let short = cow_says("Bonjour !");
        assert_eq!(short[0], " ___________");
        assert_eq!(short[1], "< Bonjour ! >");
        assert_eq!(short[2], " -----------");
        let long = cow_says(FORTUNES[21]);
        assert!(long[1].starts_with("/ ") && long[1].ends_with(" \\"));
        assert!(long.iter().any(|l| l.contains("(oo)")));
    }

    #[test]
    fn same_number_same_fortune() {
        let text = |doc: Doc| doc.preview(false);
        assert_eq!(
            text(build(Source::Builtin, false, Some(7)).unwrap()),
            text(build(Source::Builtin, false, Some(7)).unwrap())
        );
    }

    #[test]
    fn long_words_are_cut() {
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("un deux trois", 7), ["un deux", "trois"]);
    }
}
