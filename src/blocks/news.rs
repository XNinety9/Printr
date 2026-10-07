//! Revue de presse à partir de flux RSS ou Atom, sans IA : les articles sont classés
//! selon les thèmes demandés, leur place dans le flux, leur fraîcheur et leurs recoupements
//! entre flux, puis dédoublonnés.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};

use super::Ctx;
use crate::doc::{Align, Doc, Style};
use crate::draw;

/// Longueur maximale du chapô imprimé.
const SUMMARY_MAX: usize = 260;

/// Rubriques et formats écartés : directs, vidéos, podcasts, tribunes.
const NOISE_PREFIXES: &[&str] = &["en direct", "direct", "video", "podcast", "replay", "tribune", "entretien"];
const NOISE_PATHS: &[&str] = &["/live/", "/direct/", "/video/", "/videos/", "/podcast", "/replay", "/idees/", "/opinion", "/tribune"];

#[derive(Debug, Clone)]
pub struct Article {
    pub title: String,
    pub summary: String,
    pub link: String,
    pub source: String,
    pub categories: Vec<String>,
    pub published: Option<DateTime<Utc>>,
    /// Indice du flux et rang dans ce flux (0 = en tête).
    pub feed: usize,
    pub position: usize,
    pub feed_len: usize,
}

pub struct Options<'a> {
    pub count: usize,
    pub qr: usize,
    pub themes: &'a [String],
    pub exclude: &'a [String],
    pub max_age: Duration,
}

/// Minuscules sans accents, pour comparer des mots.
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'à' | 'â' | 'ä' => 'a',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'œ' => 'o',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect()
}

/// Le thème apparaît-il en début de mot ? (« climat » trouve « climatique »).
fn mentions(text: &str, theme: &str) -> bool {
    let theme = normalize(theme);
    let theme = theme.trim();
    !theme.is_empty() && format!(" {} ", normalize(text)).contains(&format!(" {theme}"))
}

/// Mots significatifs d'un titre, pour repérer deux articles sur le même sujet.
fn keywords(text: &str) -> Vec<String> {
    normalize(text).split_whitespace().filter(|w| w.chars().count() >= 5).map(str::to_owned).collect()
}

fn same_story(a: &Article, b: &Article) -> bool {
    let (ka, kb) = (keywords(&a.title), keywords(&b.title));
    ka.iter().filter(|w| kb.contains(w)).count() >= 2
}

fn is_noise(article: &Article, exclude: &[String]) -> bool {
    let title = normalize(&article.title);
    let link = article.link.to_lowercase();
    NOISE_PREFIXES.iter().any(|p| title.starts_with(p))
        || NOISE_PATHS.iter().any(|p| link.contains(p))
        || exclude.iter().any(|e| {
            mentions(&article.title, e) || mentions(&article.summary, e) || article.categories.iter().any(|c| mentions(c, e))
        })
}

/// Score d'un article : plus il est élevé, plus l'article est mis en avant.
fn score(article: &Article, all: &[Article], themes: &[String], now: DateTime<Utc>) -> f64 {
    // Place dans le flux : la tête du flux est la une de la rédaction.
    let mut score = 3.0 * (1.0 - article.position as f64 / article.feed_len.max(1) as f64);
    // Thèmes : titre > rubrique (URL, catégories) > chapô.
    let path = article.link.split_once("://").map_or(article.link.as_str(), |(_, r)| r).replace(['/', '-', '_'], " ");
    for theme in themes {
        if mentions(&article.title, theme) {
            score += 4.0;
        } else if mentions(&path, theme) || article.categories.iter().any(|c| mentions(c, theme)) {
            score += 3.0;
        } else if mentions(&article.summary, theme) {
            score += 2.0;
        }
    }
    // Recoupements : même sujet traité par d'autres flux.
    let mut feeds: Vec<usize> = all.iter().filter(|o| o.feed != article.feed && same_story(article, o)).map(|o| o.feed).collect();
    feeds.dedup();
    score += 2.0 * feeds.len() as f64;
    // Fraîcheur.
    if article.published.is_some_and(|p| now - p < Duration::hours(6)) {
        score += 1.0;
    }
    score
}

/// Choisit les articles à imprimer, dans l'ordre d'importance.
pub fn select(articles: Vec<Article>, options: &Options, now: DateTime<Utc>) -> Vec<Article> {
    let candidates: Vec<Article> = articles
        .into_iter()
        .filter(|a| !a.title.is_empty() && !a.link.is_empty())
        .filter(|a| a.published.is_none_or(|p| now - p <= options.max_age))
        .filter(|a| !is_noise(a, options.exclude))
        .collect();
    let mut ranked: Vec<(f64, &Article)> =
        candidates.iter().map(|a| (score(a, &candidates, options.themes, now), a)).collect();
    ranked.sort_by(|x, y| y.0.total_cmp(&x.0));
    if std::env::var_os("PRINTR_DEBUG").is_some() {
        for (s, a) in &ranked {
            eprintln!("[news] {s:5.1}  {}  ({})", a.title, a.source);
        }
    }

    let mut chosen: Vec<Article> = Vec::new();
    for (_, article) in ranked {
        if chosen.len() >= options.count {
            break;
        }
        if !chosen.iter().any(|c| same_story(c, article)) {
            chosen.push(article.clone());
        }
    }
    chosen
}

/// Retire les balises HTML et les entités courantes d'un chapô.
fn strip_html(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                text.push(' ');
            }
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    // Pas d'espace avant la ponctuation finale laissée par une balise fermante.
    text.split_whitespace().collect::<Vec<_>>().join(" ").replace(" .", ".")
}

/// Coupe un chapô trop long à la fin d'une phrase, sinon au dernier mot.
fn shorten(text: &str) -> String {
    if text.chars().count() <= SUMMARY_MAX {
        return text.to_owned();
    }
    let cut: String = text.chars().take(SUMMARY_MAX).collect();
    if let Some(end) = cut.rfind(['.', '!', '?']).filter(|&i| i > SUMMARY_MAX / 2) {
        return cut[..=end].to_owned();
    }
    match cut.rfind(' ') {
        Some(i) => format!("{}...", &cut[..i]),
        None => cut,
    }
}

/// « www.franceinfo.fr » -> « franceinfo.fr ».
fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest).trim_start_matches("www.").to_owned()
}

/// Télécharge et lit un flux RSS ou Atom.
fn fetch(agent: &ureq::Agent, url: &str, index: usize) -> Result<Vec<Article>> {
    let mut response = agent
        .get(url)
        .header("User-Agent", "printr/0.1 (imprimante à tickets personnelle)")
        .call()
        .with_context(|| format!("flux injoignable : {url}"))?;
    let bytes = response.body_mut().with_config().limit(10 * 1024 * 1024).read_to_vec()?;
    let feed = feed_rs::parser::parse(bytes.as_slice()).with_context(|| format!("flux illisible : {url}"))?;
    let source = host(feed.links.first().map_or(url, |l| l.href.as_str()));
    let feed_len = feed.entries.len();
    Ok(feed
        .entries
        .into_iter()
        .enumerate()
        .map(|(position, entry)| {
            let summary = entry
                .summary
                .map(|t| t.content)
                .or_else(|| entry.content.and_then(|c| c.body))
                .unwrap_or_default();
            Article {
                title: strip_html(&entry.title.map(|t| t.content).unwrap_or_default()),
                summary: shorten(&strip_html(&summary)),
                link: entry.links.first().map(|l| l.href.clone()).unwrap_or_default(),
                source: source.clone(),
                categories: entry.categories.into_iter().map(|c| c.label.unwrap_or(c.term)).collect(),
                published: entry.published.or(entry.updated),
                feed: index,
                position,
                feed_len,
            }
        })
        .collect())
}

pub fn build(ctx: &Ctx, title: &str, feeds: &[String], options: &Options) -> Result<Doc> {
    if feeds.is_empty() {
        bail!("aucun flux RSS (`feeds`)");
    }
    // Les flux sont téléchargés en parallèle ; un flux en panne n'empêche pas les autres.
    let results: Vec<Result<Vec<Article>>> = std::thread::scope(|s| {
        let handles: Vec<_> =
            feeds.iter().enumerate().map(|(i, url)| s.spawn(move || fetch(&ctx.http, url, i))).collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| bail!("erreur interne"))).collect()
    });
    let mut articles = Vec::new();
    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok(list) => articles.extend(list),
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    if articles.is_empty() {
        bail!("{}", if errors.is_empty() { "aucun article".to_owned() } else { errors.join(" ; ") });
    }

    let chosen = select(articles, options, Utc::now());
    if chosen.is_empty() {
        bail!("aucun article récent ne correspond");
    }

    let mut doc = Doc::new();
    doc.header(&format!("Actualités · {title}"));
    let qr = options.qr.min(2).min(chosen.len());
    for (i, article) in chosen.iter().enumerate() {
        doc.feed(1);
        let marker = if i < qr { format!("[{}] ", i + 1) } else { String::new() };
        doc.hanging(&marker, &article.title, Style::default().bold());
        if !article.summary.is_empty() {
            doc.text(&article.summary, Style::default());
        }
        doc.text(&format!("- {}", article.source), Style::default().small().align(Align::Right));
    }
    if qr > 0 {
        doc.feed(1);
        let urls: Vec<&str> = chosen[..qr].iter().map(|a| a.link.as_str()).collect();
        doc.image(draw::qr_row(&urls)?);
        // Légendes centrées sous chaque QR code, en police B (deux colonnes de 28 caractères).
        let style = Style::default().small();
        let half = style.columns() / qr;
        let caption: String = chosen[..qr]
            .iter()
            .enumerate()
            .map(|(i, a)| format!("{:^half$}", format!("[{}] {}", i + 1, a.source)))
            .collect();
        doc.line(&caption, style);
    }
    if !errors.is_empty() {
        doc.text(&format!("({} flux en panne)", errors.len()), Style::default().small().center());
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn article(title: &str, link: &str, feed: usize, position: usize) -> Article {
        Article {
            title: title.to_owned(),
            summary: String::new(),
            link: link.to_owned(),
            source: "test".to_owned(),
            categories: Vec::new(),
            published: None,
            feed,
            position,
            feed_len: 10,
        }
    }

    fn options<'a>(themes: &'a [String], exclude: &'a [String]) -> Options<'a> {
        Options { count: 3, qr: 2, themes, exclude, max_age: Duration::hours(24) }
    }

    #[test]
    fn themes_beat_feed_order() {
        let articles = vec![
            article("Le budget examiné à l'Assemblée", "https://a.fr/politique/budget", 0, 0),
            article("Les glaciers fondent plus vite que prévu", "https://a.fr/environnement/glaciers", 0, 5),
        ];
        let themes = vec!["climat".to_owned(), "environnement".to_owned()];
        let chosen = select(articles, &options(&themes, &[]), Utc::now());
        assert!(chosen[0].title.contains("glaciers"));
    }

    #[test]
    fn filters_noise_exclusions_and_old_news() {
        let mut old = article("Vieille nouvelle importante", "https://a.fr/societe/vieux", 0, 0);
        old.published = Some(Utc::now() - Duration::days(3));
        let articles = vec![
            article("EN DIRECT, allocution du Premier ministre", "https://a.fr/politique/live/x", 0, 0),
            article("Résultats de football du week-end", "https://a.fr/sport/foot", 0, 1),
            old,
            article("Un article normal sur la santé", "https://a.fr/sante/article", 0, 2),
        ];
        let exclude = vec!["football".to_owned()];
        let chosen = select(articles, &options(&[], &exclude), Utc::now());
        assert_eq!(chosen.len(), 1);
        assert!(chosen[0].title.contains("santé"));
    }

    #[test]
    fn cross_feed_stories_rank_higher_and_are_deduplicated() {
        let articles = vec![
            article("Un fait divers local", "https://a.fr/faits-divers/x", 0, 0),
            article("Mobilisation des lycéens contre la réforme", "https://a.fr/education/x", 0, 3),
            article("La mobilisation des lycéens s'amplifie", "https://b.fr/societe/y", 1, 2),
        ];
        let chosen = select(articles, &options(&[], &[]), Utc::now());
        assert!(chosen[0].title.contains("lycéens"));
        assert_eq!(chosen.iter().filter(|a| a.title.contains("lycéens")).count(), 1);
    }

    #[test]
    fn parses_rss_and_cleans_text() {
        let rss = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>Test</title>
            <link>https://www.exemple.fr/</link>
            <item><title>Le prix de l&#xE9;lectricit&#xE9;</title>
            <link>https://www.exemple.fr/eco/prix.html</link>
            <description>&lt;p&gt;Un &lt;b&gt;chapô&lt;/b&gt; court.&lt;/p&gt;</description></item>
            </channel></rss>"#;
        let feed = feed_rs::parser::parse(rss.as_bytes()).unwrap();
        let entry = &feed.entries[0];
        assert_eq!(entry.title.as_ref().unwrap().content, "Le prix de lélectricité");
        assert_eq!(strip_html(&entry.summary.as_ref().unwrap().content), "Un chapô court.");
        assert_eq!(host("https://www.exemple.fr/eco/prix.html"), "exemple.fr");
    }

    #[test]
    fn long_summaries_are_cut_cleanly() {
        let text = "Première phrase assez longue pour remplir. ".repeat(10);
        let short = shorten(&text);
        assert!(short.chars().count() <= SUMMARY_MAX + 3);
        assert!(short.ends_with('.'));
    }
}
