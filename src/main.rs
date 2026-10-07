use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::builder::styling::{AnsiColor, Styles};
use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};

mod blocks;
mod cache;
mod claude;
mod cp858;
mod doc;
mod draw;
mod fr;
mod output;
mod raster;
mod scheduler;
mod server;
mod store;
mod ui;

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
    #[arg(short, long, value_name = "CHEMIN", default_value = "/dev/usb/lp0", env = "PRINTR_DEVICE",
          hide_default_value = true, hide_env = true)]
    device: PathBuf,

    /// Écrit les octets ESC/POS dans ce fichier au lieu de l'imprimante
    #[arg(long, value_name = "FICHIER", conflicts_with_all = ["device", "tcp"])]
    dump: Option<PathBuf>,

    /// Envoie en TCP (imprimante réseau ou émulateur), ex. 127.0.0.1:9100
    #[arg(long, value_name = "HÔTE:PORT", conflicts_with = "device")]
    tcp: Option<String>,

    /// Affiche un aperçu dans le terminal au lieu d'imprimer
    #[arg(long, global = true)]
    preview: bool,

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
        #[arg(long, value_name = "ADRESSE", default_value = "0.0.0.0:8080", env = "PRINTR_LISTEN",
              hide_default_value = true, hide_env = true)]
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
        /// Seuil noir/blanc au lieu du tramage (logos, dessins au trait)
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
        let first = rpassword::prompt_password(format!("Mot de passe pour {name} : "))?;
        let again = rpassword::prompt_password("Confirmation : ")?;
        anyhow::ensure!(first == again, "les deux mots de passe diffèrent");
        Ok(first)
    };
    let done = match action {
        UserAction::Add { name } => {
            let password = ask(name)?;
            store.add_user(name, &password)?;
            format!("compte « {name} » créé")
        }
        UserAction::Passwd { name } => {
            let password = ask(name)?;
            store.set_password(name, &password)?;
            format!("mot de passe de « {name} » changé")
        }
        UserAction::Remove { name } => {
            store.remove_user(name)?;
            format!("compte « {name} » supprimé")
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
    let template = format!("{{about-with-newline}}\n{heading}Utilisation :{heading:#} {{usage}}\n\n{{all-args}}");
    // Titres de sections à la française : « Options : », « Arguments : ».
    let french_headings = |cmd: clap::Command| {
        cmd.help_template(template.clone()).mut_args(|arg| {
            if arg.get_help_heading().is_some() {
                arg
            } else if arg.is_positional() {
                arg.help_heading("Arguments ")
            } else {
                arg.help_heading("Options ")
            }
        })
    };
    let command = french_headings(Cli::command()).mut_subcommands(french_headings);
    let matches = command.get_matches();
    Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
}

fn main() -> ExitCode {
    match run(parse_cli()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            ui::error(&e);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let start = Instant::now();
    let target = match (&cli.tcp, &cli.dump) {
        (Some(addr), _) => Target::Tcp(addr.clone()),
        (_, Some(path)) => Target::Dump(path.clone()),
        _ => Target::Device(cli.device.clone()),
    };

    // On prépare tout le ticket (y compris les requêtes réseau) avant d'ouvrir l'imprimante.
    let mut reports = Vec::new();
    let (doc, cut) = match &cli.command {
        Command::Serve { listen, token } => {
            if let Some(token) = token {
                anyhow::ensure!(token.len() >= 16, "le jeton doit faire au moins 16 caractères");
            }
            return server::serve(listen, token.clone(), target);
        }
        Command::User { action } => return user_command(action),
        Command::Print { file } => {
            let json = match file {
                Some(path) if path.as_os_str() != "-" => std::fs::read_to_string(path)
                    .with_context(|| format!("impossible de lire {}", path.display()))?,
                _ => read_stdin()?,
            };
            let ticket: blocks::Ticket = serde_json::from_str(&json).context("ticket JSON invalide")?;
            let mut progress: Box<dyn blocks::Progress> =
                if cli.quiet { Box::new(blocks::Silent) } else { Box::new(ui::Console::new()) };
            let (doc, ticket_reports) = ticket.build(&blocks::Ctx::new(cli.refresh), progress.as_mut());
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
        Command::Image { path, no_dither, no_cut } => {
            let mut doc = Doc::new();
            doc.image(raster::load(path, !no_dither)?);
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
        .text("Ticket de test", base.center())
        .feed(1)
        .header("Styles")
        .text("Normal", base)
        .text("Gras", base.bold())
        .text("Souligné", base.underline())
        .text(" Inversé ", base.reverse())
        .text("Police B, plus petite", base.small())
        .text("Double taille", base.size(2))
        .feed(1)
        .header("Caractères")
        .text("Accents : éèêàçùôï ÉÀÇÒ € ° « »", base)
        .text("Typographie : l’œuf… – fin", base)
        .text("123456789012345678901234567890123456789012", base)
        .rule('─')
        .feed(1)
        .qr("https://github.com/fabienbellanger/escpos-rs", 6);
    doc
}
