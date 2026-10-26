use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::builder::styling::{AnsiColor, Styles};
use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};

// Déclaré en premier :  est visible dans tous les modules qui suivent (macro_use).
#[macro_use]
mod i18n;
mod backup;
mod blocks;
mod cache;
mod claude;
mod cp858;
mod detect;
mod doc;
mod draw;
mod guard;
mod hass;
mod mqtt;
mod net;
mod output;
mod raster;
mod scheduler;
mod server;
mod settings;
mod store;
mod ui;
mod update;
mod wifi;

use crate::i18n::t;
use doc::{Doc, Style};
use output::Target;

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Yellow.on_default().bold())
    .usage(AnsiColor::Yellow.on_default().bold())
    .literal(AnsiColor::Green.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Red.on_default().bold())
    .error(AnsiColor::Red.on_default().bold());

/// Nom de la version mineure en cours : un pionnier de l'imprimerie, par ordre alphabétique (voir
/// le CHANGELOG). À changer à chaque nouvelle version mineure, en même temps que le CHANGELOG.
const CODENAME: &str = "Mergenthaler";

/// Imprime des tickets sur une Epson TM-T88V (ESC/POS).
#[derive(Parser)]
#[command(
    version,
    styles = STYLES,
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true,
    // L'espace finale donne « Commandes : » (clap ajoute les deux-points).
    subcommand_help_heading = "Commandes ",
    subcommand_value_name = "COMMANDE"
)]
struct Cli {
    /// Périphérique de l'imprimante (défaut /dev/usb/lp0, ou $PRINTR_DEVICE)
    // `PRINTR_DEVICE` est lu à la main dans `target()` : venant de l'environnement, il ne doit pas
    // entrer en conflit avec `--dump` ou `--tcp`, qui l'emportent.
    #[arg(short, long, value_name = "CHEMIN")]
    device: Option<PathBuf>,

    /// Écrit les octets ESC/POS dans ce fichier au lieu de l'imprimante
    #[arg(long, value_name = "FICHIER", conflicts_with_all = ["device", "tcp"])]
    dump: Option<PathBuf>,

    /// Envoie en TCP (imprimante réseau ou émulateur), ex. 127.0.0.1:9100, ou $PRINTR_TCP
    #[arg(long, value_name = "HÔTE:PORT", conflicts_with = "device")]
    tcp: Option<String>,

    /// Affiche un aperçu dans le terminal au lieu d'imprimer (sans appel payant à Claude)
    #[arg(long, global = true)]
    preview: bool,

    /// Avec --preview : fait quand même rédiger les blocs Claude (payant)
    #[arg(long, global = true, requires = "preview")]
    claude: bool,

    /// Régénère les actualités, les horaires du soleil, l'horoscope et le mot du jour
    #[arg(long, global = true)]
    refresh: bool,

    /// N'affiche que les erreurs
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Affiche l'aide
    #[arg(short = 'h', long, action = ArgAction::Help, global = true)]
    help: Option<bool>,

    /// Affiche la version
    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Imprime un ticket décrit en JSON (fichier, ou entrée standard si absent ou « - »)
    Print {
        /// Fichier JSON du ticket
        #[arg(value_name = "TICKET")]
        file: Option<PathBuf>,
    },
    /// Lance l'interface web et l'API (avec le planificateur)
    Serve {
        /// Adresse d'écoute (défaut 0.0.0.0:8080, ou $PRINTR_LISTEN)
        #[arg(
            long,
            value_name = "ADRESSE",
            default_value = "0.0.0.0:8080",
            env = "PRINTR_LISTEN",
            hide_default_value = true,
            hide_env = true
        )]
        listen: String,
        /// Jeton pour les scripts : « Authorization: Bearer <jeton> » (ou $PRINTR_TOKEN)
        #[arg(long, value_name = "JETON", env = "PRINTR_TOKEN", hide_env = true)]
        token: Option<String>,
    },
    /// Gère les comptes de l'interface web
    User {
        #[command(subcommand)]
        action: UserAction,
    },
    /// Imprime un ticket de test (styles, accents, QR code)
    Test,
    /// Imprime du texte (lit l'entrée standard si aucun texte n'est donné)
    Text {
        /// Texte à imprimer
        #[arg(value_name = "TEXTE")]
        text: Vec<String>,
        /// Ne coupe pas le papier à la fin
        #[arg(long)]
        no_cut: bool,
    },
    /// Imprime une image (réduite à 512 px de large, tramée)
    Image {
        /// Fichier image (PNG, JPEG, GIF…)
        #[arg(value_name = "IMAGE")]
        path: PathBuf,
        /// Tramage : floyd-steinberg (défaut, photos), atkinson (plus contrasté), bayer (trame régulière) ou none
        #[arg(
            long,
            value_enum,
            default_value_t = raster::Dither::FloydSteinberg,
            value_name = "ALGO",
            hide_possible_values = true,
            hide_default_value = true
        )]
        dither: raster::Dither,
        /// Seuil noir/blanc au lieu du tramage (logos, dessins au trait), comme `--dither none`
        #[arg(long)]
        no_dither: bool,
        /// Ne coupe pas le papier à la fin
        #[arg(long)]
        no_cut: bool,
    },
}

#[derive(Subcommand)]
enum UserAction {
    /// Crée un compte (le mot de passe est demandé)
    Add {
        #[arg(value_name = "PRÉNOM")]
        name: String,
    },
    /// Change le mot de passe d'un compte
    Passwd {
        #[arg(value_name = "PRÉNOM")]
        name: String,
    },
    /// Supprime un compte et ses presets
    Remove {
        #[arg(value_name = "PRÉNOM")]
        name: String,
    },
    /// Liste les comptes
    List,
}

fn user_command(action: &UserAction) -> Result<()> {
    let mut store = store::Store::open()?;
    let ask = |name: &str| -> Result<String> {
        // Hors terminal (script de déploiement), le mot de passe est lu sur l'entrée standard.
        if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            return Ok(line.trim_end_matches(['\r', '\n']).to_owned());
        }
        let first = rpassword::prompt_password(tr!("Mot de passe pour {name} : ", "Password for {name}: "))?;
        let again = rpassword::prompt_password(t("Confirmation : ", "Confirm: "))?;
        anyhow::ensure!(first == again, "{}", t("les deux mots de passe diffèrent", "the two passwords differ"));
        Ok(first)
    };
    let done = match action {
        UserAction::Add { name } => {
            let password = ask(name)?;
            store.add_user(name, &password)?;
            tr!("compte « {name} » créé", "account \"{name}\" created")
        }
        UserAction::Passwd { name } => {
            let password = ask(name)?;
            store.set_password(name, &password)?;
            tr!("mot de passe de « {name} » changé", "password of \"{name}\" changed")
        }
        UserAction::Remove { name } => {
            store.remove_user(name)?;
            tr!("compte « {name} » supprimé", "account \"{name}\" deleted")
        }
        UserAction::List => {
            for user in store.users() {
                anstream::println!("{}", user.name);
            }
            return Ok(());
        }
    };
    store.save()?;
    anstream::eprintln!("{}✓{} {done}", anstyle::AnsiColor::Green.on_default().bold(), anstyle::Reset);
    Ok(())
}

/// Analyse la ligne de commande, avec une aide en français.
fn parse_cli() -> Cli {
    let heading = STYLES.get_usage();
    let template = format!(
        "{{about-with-newline}}\n{heading}{}{heading:#} {{usage}}\n\n{{all-args}}",
        t("Utilisation :", "Usage:")
    );
    // Titres de sections à la française : « Options : », « Arguments : ».
    let headings = |cmd: clap::Command| {
        cmd.help_template(template.clone()).mut_args(|arg| {
            if arg.get_help_heading().is_some() {
                arg
            } else if arg.is_positional() {
                arg.help_heading(t("Arguments ", "Arguments"))
            } else {
                arg.help_heading(t("Options ", "Options"))
            }
        })
    };
    // « printr 1.6.0 “Gutenberg” » ; une seule chaîne, construite une fois.
    let version: &'static str = Box::leak(format!("{} “{CODENAME}”", env!("CARGO_PKG_VERSION")).into_boxed_str());
    let mut command = headings(Cli::command().version(version)).mut_subcommands(headings);
    if i18n::en() {
        command = in_english(command);
    }
    let matches = command.get_matches();
    Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
}

/// L'aide de clap vient des commentaires `///`, en français : en anglais, on la remplace, commande
/// par commande et option par option.
fn in_english(command: clap::Command) -> clap::Command {
    fn about(name: &str) -> Option<&'static str> {
        Some(match name {
            "printr" => "Prints tickets on an Epson TM-T88V (ESC/POS).",
            "print" => "Prints a ticket described in JSON (file, or standard input if absent or \"-\")",
            "serve" => "Starts the web app and the API (with the scheduler)",
            "user" => "Manages the web app accounts",
            "test" => "Prints a test ticket (styles, accents, QR code)",
            "text" => "Prints text (reads standard input if no text is given)",
            "image" => "Prints an image (scaled to 512 px wide, dithered)",
            "add" => "Creates an account (asks for the password)",
            "passwd" => "Changes an account's password",
            "remove" => "Deletes an account and its presets",
            "list" => "Lists the accounts",
            _ => return None,
        })
    }
    fn arg(id: &str) -> Option<(&'static str, Option<&'static str>)> {
        Some(match id {
            "device" => ("Printer device (default /dev/usb/lp0, or $PRINTR_DEVICE)", Some("PATH")),
            "dump" => ("Writes the ESC/POS bytes to this file instead of the printer", Some("FILE")),
            "tcp" => {
                ("Sends over TCP (network printer or emulator), e.g. 127.0.0.1:9100, or $PRINTR_TCP", Some("HOST:PORT"))
            }
            "preview" => ("Shows a preview in the terminal instead of printing (no paid Claude call)", None),
            "claude" => ("With --preview: still have Claude write its blocks (paid)", None),
            "refresh" => ("Regenerates the news, sun times, horoscope and word of the day", None),
            "quiet" => ("Only shows errors", None),
            "help" => ("Shows help", None),
            "version" => ("Shows the version", None),
            "file" => ("JSON ticket file", None),
            "listen" => ("Listening address (default 0.0.0.0:8080, or $PRINTR_LISTEN)", Some("ADDRESS")),
            "token" => ("Token for scripts: \"Authorization: Bearer <token>\" (or $PRINTR_TOKEN)", Some("TOKEN")),
            "text" => ("Text to print", Some("TEXT")),
            "no_cut" => ("Doesn't cut the paper at the end", None),
            "path" => ("Image file (PNG, JPEG, GIF…)", None),
            "dither" => (
                "Dithering: floyd-steinberg (default, photos), atkinson (more contrast), bayer (regular pattern) or none",
                Some("ALGO"),
            ),
            "no_dither" => ("Black/white threshold instead of dithering (logos, line art), like `--dither none`", None),
            "name" => ("", Some("NAME")),
            _ => return None,
        })
    }
    fn translate(mut cmd: clap::Command) -> clap::Command {
        if let Some(text) = about(cmd.get_name()) {
            cmd = cmd.about(text);
        }
        cmd = cmd.subcommand_help_heading("Commands").subcommand_value_name("COMMAND").mut_args(|a| {
            let Some((help, value)) = arg(a.get_id().as_str()) else { return a };
            let a = if help.is_empty() { a } else { a.help(help) };
            match value {
                Some(v) => a.value_name(v),
                None => a,
            }
        });
        cmd.mut_subcommands(translate)
    }
    translate(command)
}

/// Configuration du service sur le Pi, lue aussi par la commande `printr` : un ticket lancé à la
/// main a la même clé Claude, la même imprimante et le même Barnum que le serveur.
const SYSTEM_ENV: &str = "/etc/printr.env";

/// Lignes `CLÉ=valeur` d'un fichier d'environnement (format systemd), guillemets retirés.
fn parse_env_file(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| {
            let v = v.trim();
            let unquoted = [('"', '"'), ('\'', '\'')]
                .iter()
                .find_map(|&(a, b)| v.strip_prefix(a).and_then(|v| v.strip_suffix(b)))
                .unwrap_or(v);
            (k.trim().to_owned(), unquoted.to_owned())
        })
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// Charge `/etc/printr.env` s'il est lisible, sans écraser les variables déjà définies.
fn load_system_env() {
    let Ok(text) = std::fs::read_to_string(SYSTEM_ENV) else { return };
    for (key, value) in parse_env_file(&text) {
        if std::env::var_os(&key).is_none() && !value.is_empty() {
            // SAFETY : appelé au tout début de `main`, avant le moindre fil d'exécution.
            unsafe { std::env::set_var(key, value) };
        }
    }
}

fn main() -> ExitCode {
    load_system_env();
    // Puis les réglages faits dans l'appli, pour ce que le système n'a pas fixé.
    settings::load(store::data_dir().map(|d| d.join("printr.json")).as_deref());
    // CrossWords (mots fléchés) range son dictionnaire et ses définitions dans le cache de printr,
    // seul dossier où le service a le droit d'écrire.
    if std::env::var_os("CROSSWORDS_CACHE").is_none()
        && let Some(cache) = cache::Cache::open()
    {
        // SAFETY : au tout début de `main`, avant le moindre fil d'exécution.
        unsafe { std::env::set_var("CROSSWORDS_CACHE", cache.dir().join("crosswords")) };
    }
    match run(parse_cli()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            ui::error(&e);
            ExitCode::FAILURE
        }
    }
}

/// Destination des octets : `--tcp`, `--dump` ou `--device` ; sinon `$PRINTR_TCP` (imprimante
/// réseau), `$PRINTR_DEVICE`, ou le lien /dev/tm88 de la règle udev s'il existe, sinon
/// /dev/usb/lp0. Les options l'emportent sur l'environnement.
fn target(cli: &Cli) -> Target {
    let env = |name| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    match (&cli.tcp, &cli.dump, &cli.device) {
        (Some(addr), _, _) => Target::Tcp(addr.clone()),
        (_, Some(path), _) => Target::Dump(path.clone()),
        (_, _, Some(device)) => Target::Device(device.clone()),
        _ => match (env("PRINTR_TCP"), env("PRINTR_DEVICE")) {
            (Some(addr), _) => Target::Tcp(addr),
            (None, Some(device)) => Target::Device(device.into()),
            (None, None) if Path::new("/dev/tm88").exists() => Target::Device("/dev/tm88".into()),
            (None, None) => Target::Device("/dev/usb/lp0".into()),
        },
    }
}

fn run(cli: Cli) -> Result<()> {
    let start = Instant::now();
    let target = target(&cli);

    // On prépare tout le ticket (y compris les requêtes réseau) avant d'ouvrir l'imprimante.
    let mut reports = Vec::new();
    let (doc, cut) = match &cli.command {
        Command::Serve { listen, token } => {
            if let Some(token) = token {
                anyhow::ensure!(
                    token.len() >= 16,
                    "{}",
                    t("le jeton doit faire au moins 16 caractères", "the token must be at least 16 characters")
                );
            }
            return server::serve(listen, token.clone(), target);
        }
        Command::User { action } => return user_command(action),
        Command::Print { file } => {
            let json = match file {
                Some(path) if path.as_os_str() != "-" => std::fs::read_to_string(path)
                    .with_context(|| tr!("impossible de lire {}", "cannot read {}", path.display()))?,
                _ => read_stdin()?,
            };
            let ticket: blocks::Ticket =
                serde_json::from_str(&json).context(t("ticket JSON invalide", "invalid ticket JSON"))?;
            let mut progress: Box<dyn blocks::Progress> =
                if cli.quiet { Box::new(blocks::Silent) } else { Box::new(ui::Console::new()) };
            // Un aperçu ne coûte rien, comme dans l'appli : Claude seulement si on le demande.
            let ctx = blocks::Ctx::new(cli.refresh);
            let ctx = if cli.preview && !cli.claude { ctx.for_preview() } else { ctx };
            let (doc, ticket_reports) = ticket.build(&ctx, progress.as_mut());
            reports = ticket_reports;
            (doc, ticket.cut)
        }
        Command::Test => (test_ticket(), true),
        Command::Text { text, no_cut } => {
            let text = if text.is_empty() { read_stdin()? } else { text.join(" ") };
            let mut doc = Doc::new();
            doc.text(&text, Style::default());
            (doc, !no_cut)
        }
        Command::Image { path, dither, no_dither, no_cut } => {
            let mut doc = Doc::new();
            doc.image(raster::load(path, if *no_dither { raster::Dither::None } else { *dither })?);
            (doc, !no_cut)
        }
    };

    let failures = reports.iter().filter(|r| r.error.is_some()).count();
    let destination = if cli.preview {
        anstream::print!("{}", doc.preview_styled(cut));
        None
    } else {
        target.send(&doc, cut)?;
        Some(target.to_string())
    };
    if !cli.quiet {
        ui::summary(&doc, reports.len(), destination.as_deref(), start.elapsed(), failures);
    }
    Ok(())
}

fn read_stdin() -> Result<String> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

fn test_ticket() -> Doc {
    let base = Style::default();
    let mut doc = Doc::new();
    doc.text("PRINTR", base.bold().center().size(2))
        .text(t("Ticket de test", "Test ticket"), base.center())
        .feed(1)
        .header("Styles")
        .text("Normal", base)
        .text(t("Gras", "Bold"), base.bold())
        .text(t("Souligné", "Underlined"), base.underline())
        .text(t(" Inversé ", " Reverse "), base.reverse())
        .text(t("Police B, plus petite", "Font B, smaller"), base.small())
        .text(t("Double taille", "Double size"), base.size(2))
        .feed(1)
        .header(t("Caractères", "Characters"))
        .text(t("Accents : éèêàçùôï ÉÀÇÒ € ° « »", "Accents: éèêàçùôï ÉÀÇÒ € ° « »"), base)
        .text(t("Typographie : l’œuf… – fin", "Typography: l’œuf… – fin"), base)
        .text("123456789012345678901234567890123456789012", base)
        .rule('─')
        .feed(1)
        .qr("https://github.com/fabienbellanger/escpos-rs", 6);
    doc
}

#[cfg(test)]
mod tests {
    /// Le nom de code est celui que le CHANGELOG donne à cette version (s'il la connaît déjà).
    #[test]
    fn codename_matches_changelog() {
        let heading = format!("## [{}]", env!("CARGO_PKG_VERSION"));
        if let Some(line) = include_str!("../CHANGELOG.md").lines().find(|l| l.starts_with(&heading)) {
            assert!(line.contains(&format!("“{CODENAME}”")), "CODENAME = {CODENAME}, CHANGELOG : {line}");
        }
    }

    use super::*;

    #[test]
    fn env_file_like_systemd() {
        let text = "# commentaire\nPRINTR_BARNUM=\"python3 /opt/barnum/main.py\"\nANTHROPIC_API_KEY=\n\nPRINTR_LISTEN=0.0.0.0:8080\nX='a b'\n";
        let vars = parse_env_file(text);
        assert_eq!(vars[0], ("PRINTR_BARNUM".into(), "python3 /opt/barnum/main.py".into()));
        assert_eq!(vars[1], ("ANTHROPIC_API_KEY".into(), String::new()));
        assert_eq!(vars[2].1, "0.0.0.0:8080");
        assert_eq!(vars[3].1, "a b");
    }
}
