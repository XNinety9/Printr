"""Construit le site dans _site/ : la page d'accueil (docs/) et les pages de documentation,
rendues depuis le guide (docs/guide.md) et le guide de contribution du dépôt pour ne jamais s'en écarter.

    uvx --with markdown --with pymdown-extensions python docs/build.py && python3 -m http.server -d _site
"""

import html
import json
import os
import re
import shutil
import struct
import unicodedata
from pathlib import Path

import markdown
from pymdownx.slugs import slugify

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "docs"
OUT = ROOT / "_site"
REPO = "https://github.com/XNinety9/Printr"
BRANCH = "master"

# (page produite, langue, titre, source, page jumelle dans l'autre langue)
PAGES = [
    ("guide.html", "fr", "Guide", SITE / "guide.fr.md", "guide.en.html"),
    ("contribuer.html", "fr", "Contribuer", ROOT / "CONTRIBUTING.fr.md", "contributing.html"),
    ("guide.en.html", "en", "Guide", SITE / "guide.md", "guide.html"),
    ("contributing.html", "en", "Contributing", ROOT / "CONTRIBUTING.md", "contribuer.html"),
]
# Page des blocs (bilingue), générée depuis le catalogue et les exemples de docs/exemples/.
BLOCKS_PAGE = "blocs.html"
CATALOGUE = SITE / "exemples" / "catalogue.json"
# Navigation des pages de documentation, par langue : la page des blocs, puis les pages Markdown.
NAV = {
    lang: [(BLOCKS_PAGE, "Blocs" if lang == "fr" else "Blocks")]
    + [(page, title) for page, page_lang, title, _, _ in PAGES if page_lang == lang]
    for lang in ("fr", "en")
}
# Fichiers du dépôt qui ont leur page sur le site.
PAGE_FOR = {source.relative_to(ROOT).as_posix(): page for page, _, _, source, _ in PAGES}
# Textes des pages Markdown, par langue.
WORDS = {
    "fr": {"on_page": "Sur cette page", "generated": "Page générée depuis", "doc": "Documentation de Printr", "switch": "EN",
           "switch_label": "English version"},
    "en": {"on_page": "On this page", "generated": "Generated from", "doc": "Printr documentation", "switch": "FR",
           "switch_label": "Version française"},
}
# Ce qui, dans docs/, ne sert qu'à fabriquer le site ou ses illustrations.
SKIP = shutil.ignore_patterns("build.py", "demo", "*.sh", "*.py", "*.md", "catalogue.json", "__pycache__")

TEMPLATE = """<!DOCTYPE html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Printr</title>
<meta name="description" content="{doc} : {title_lower}.">
<link rel="alternate" hreflang="{other_lang}" href="{twin}">
<link rel="icon" href="mark.png" type="image/png">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500;700&display=swap" rel="stylesheet">
<script>
  // Thème choisi sur la page d'accueil, avant le premier affichage.
  try {{ var t = localStorage.getItem("printr-theme"); if (t === "dark" || t === "light") document.documentElement.setAttribute("data-theme", t); }} catch (e) {{}}
  // Langue choisie explicitement ailleurs sur le site : on passe à la page jumelle.
  try {{ var l = localStorage.getItem("printr-lang"); if ((l === "fr" || l === "en") && l !== "{lang}") location.replace("{twin}" + location.hash); }} catch (e) {{}}
  function switchLang() {{
    try {{ localStorage.setItem("printr-lang", "{other_lang}"); }} catch (e) {{}}
    location.href = "{twin}" + location.hash;
  }}
</script>
<link rel="stylesheet" href="docs.css">
</head>
<body>
<nav class="nav">
  <div class="wrap">
    <a class="brand" href="./" aria-label="Printr"><img src="mark.png" alt=""><span class="brand-name">Printr</span></a>
    <div class="links">{nav}<a href="{repo}">GitHub</a>
      <button class="toggle" type="button" onclick="switchLang()" aria-label="{switch_label}">{switch}</button></div>
  </div>
</nav>
<main class="wrap docs">
  <aside class="toc"><div class="title">{on_page}</div>{toc}</aside>
  <article class="doc">
    <h1>{title}</h1>
    {content}
    <p class="caption">{generated} <a href="{repo}/blob/{branch}/{source}">{source}</a>.</p>
  </article>
</main>
<script src="toc.js" defer></script>
</body>
</html>
"""


def readme_body(text: str) -> str:
    """Le guide sans ses liens d'en-tête ni son titre : la page du site a les siens."""
    text = re.sub(r"\A(?:\s*<p align=\"center\">.*?</p>)+\s*", "", text, flags=re.S)
    return re.sub(r"\A# .*\n", "", text)


def rewrite_links(body: str, source: Path) -> str:
    """Liens vers les autres pages Markdown : leur page du site ; images de docs/ : le site ;
    autres fichiers du dépôt : GitHub."""
    base = source.parent.relative_to(ROOT)

    def target(url: str) -> str:
        if re.match(r"^(https?:|mailto:|#|data:)", url):
            return url
        path, _, anchor = url.partition("#")
        repo_path = os.path.normpath((base / path).as_posix()).replace("\\", "/")
        suffix = f"#{anchor}" if anchor else ""
        if repo_path in PAGE_FOR:
            return PAGE_FOR[repo_path] + suffix
        if repo_path.startswith("docs/") and (ROOT / repo_path).is_file():
            return repo_path.removeprefix("docs/") + suffix
        kind = "tree" if (ROOT / repo_path).is_dir() else "blob"
        return f"{REPO}/{kind}/{BRANCH}/{repo_path}{suffix}"

    def fix(match: re.Match) -> str:
        attr, url = match.group(1), html.unescape(match.group(2))
        if attr == "srcset":
            url = ", ".join(" ".join([target(p.split()[0]), *p.split()[1:]]) for p in url.split(","))
        else:
            url = target(url)
        return f'{attr}="{html.escape(url)}"'

    return re.sub(r'(href|src|srcset)="([^"]+)"', fix, body)


def nav_links(current: str, lang: str) -> str:
    return "".join(f'<a href="{p}"{" aria-current=\"page\"" if p == current else ""}>{label}</a>' for p, label in NAV[lang])


def nav_links_both(current: str) -> str:
    """Navigation de la page bilingue : un jeu de liens par langue, affiché selon la langue."""
    return "".join(
        f'<a class="{lang}" href="{p}"{" aria-current=\"page\"" if p == current else ""}>{label}</a>'
        for lang in ("fr", "en")
        for p, label in NAV[lang]
    )


def render(page: str, lang: str, title: str, source: Path, twin: str) -> str:
    text = source.read_text()
    if source.name.startswith("README"):
        text = readme_body(text)
    else:
        # Le titre et le lien vers l'autre langue : la page a les siens.
        text = re.sub(r"^# .*\n", "", text, count=1)
        text = re.sub(r"\A\s*\[[^\]]*\]\((?:CONTRIBUTING|README)[^)]*\)\s*\n", "", text)
    md = markdown.Markdown(
        # superfences : blocs de code dans les listes ; md_in_html : Markdown dans le HTML du README.
        extensions=["pymdownx.superfences", "tables", "toc", "sane_lists", "attr_list", "md_in_html"],
        extension_configs={
            # Ancres à la GitHub (accents gardés) : les liens internes du README restent valides.
            "toc": {"permalink": "#", "toc_depth": "2-3", "slugify": slugify(case="lower")},
        },
    )
    body = rewrite_links(md.convert(text), source)
    return TEMPLATE.format(
        lang=lang,
        other_lang="en" if lang == "fr" else "fr",
        twin=twin,
        **WORDS[lang],
        title=title,
        title_lower=title.lower(),
        nav=nav_links(page, lang),
        # Le sommaire de Python-Markdown a son propre <div class="toc"> : la page fournit le sien.
        toc=re.sub(r'\A\s*<div class="toc">\s*|\s*</div>\s*\Z', "", md.toc),
        content=body,
        repo=REPO,
        branch=BRANCH,
        source=source.relative_to(ROOT).as_posix(),
    )


BLOCKS_TEMPLATE = """<!DOCTYPE html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Les blocs · Printr</title>
<meta name="description" content="Les {count} blocs de Printr, chacun avec le ticket qu'il imprime et l'exemple JSON qui l'a produit.">
<link rel="icon" href="mark.png" type="image/png">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500;700&display=swap" rel="stylesheet">
<script>
  // Langue et thème choisis sur la page d'accueil, avant le premier affichage.
  (function () {{
    var root = document.documentElement, lang = null, theme = null;
    try {{ lang = localStorage.getItem("printr-lang"); theme = localStorage.getItem("printr-theme"); }} catch (e) {{}}
    if (lang !== "en" && lang !== "fr") lang = /^fr\\b/i.test(navigator.language || "") ? "fr" : "en";
    root.lang = lang;
    if (theme === "dark" || theme === "light") root.setAttribute("data-theme", theme);
  }})();
  function toggleLang() {{
    var root = document.documentElement;
    root.lang = root.lang === "fr" ? "en" : "fr";
    try {{ localStorage.setItem("printr-lang", root.lang); }} catch (e) {{}}
  }}
</script>
<link rel="stylesheet" href="docs.css">
</head>
<body class="bilingual">
<nav class="nav">
  <div class="wrap">
    <a class="brand" href="./" aria-label="Printr"><img src="mark.png" alt=""><span class="brand-name">Printr</span></a>
    <div class="links">{nav}<a href="{repo}">GitHub</a>
      <button class="toggle" type="button" onclick="toggleLang()" aria-label="Français / English"><span class="fr">EN</span><span class="en">FR</span></button></div>
  </div>
</nav>
<main class="wrap docs">
  <aside class="toc"><div class="title"><span class="fr">Les blocs</span><span class="en">Blocks</span></div>{toc}</aside>
  <article class="doc">
    <h1><span class="fr">Les {count} blocs</span><span class="en">The {count} blocks</span></h1>
    <p class="fr">Chaque ticket ci-dessous sort de l'émulateur de la TM-T88V, au point près, à partir de l'exemple JSON indiqué (données fictives). Dans l'appli, on les ajoute et on les règle sans écrire de JSON. Les blocs marqués Claude font rédiger leur texte par Claude, à l'impression seulement.</p>
    <p class="fr">À côté de chaque ticket : son JSON, à envoyer tel quel à l'API, et en dessous les valeurs que chaque champ à choix accepte. Les noms anglais marchent aussi.</p>
    <p class="en">Each ticket below comes out of the TM-T88V emulator, dot for dot, from the JSON example shown (made-up data), with <code>PRINTR_LANG=en</code>. In the app, blocks are added and tuned without writing any JSON. Blocks marked Claude have their text written by Claude, at print time only.</p>
    <p class="en">Next to each ticket: its JSON, ready to send to the API as is, and below it the values each choice field accepts. The French names work too.</p>
    {content}
    <p class="caption"><span class="fr">Page générée depuis</span><span class="en">Generated from</span> <a href="{repo}/tree/{branch}/docs/exemples">docs/exemples</a>.</p>
  </article>
</main>
<script src="toc.js" defer></script>
</body>
</html>
"""


def png_size(path: Path) -> tuple[int, int]:
    """Largeur et hauteur d'un PNG (en-tête IHDR), pour réserver sa place dans la page."""
    with path.open("rb") as f:
        head = f.read(24)
    return struct.unpack(">II", head[16:24])


def both(fr: str, en: str) -> str:
    return f'<span class="fr">{html.escape(fr)}</span><span class="en">{html.escape(en)}</span>'


SRC = ROOT / "src"


def _rename(name: str, rule: str | None) -> str:
    """Nom JSON d'une variante d'après `rename_all` (serde)."""
    words = re.findall(r"[A-Z][a-z0-9]*", name) or [name]
    if rule == "kebab-case":
        return "-".join(w.lower() for w in words)
    if rule == "lowercase":
        return name.lower()
    if rule == "snake_case":
        return "_".join(w.lower() for w in words)
    return name


def _enum_body(text: str, start: int) -> str:
    """Le corps entre accolades qui commence à `start`."""
    depth, i = 0, text.index("{", start)
    for j in range(i, len(text)):
        depth += {"{": 1, "}": -1}.get(text[j], 0)
        if depth == 0:
            return text[i + 1:j]
    raise ValueError("accolade non fermée")


def _variants(body: str, rule: str | None) -> list[dict]:
    """Les variantes d'un enum : nom JSON, alias, valeur par défaut, champs (pour `Block`)."""
    out, attrs, depth = [], [], 0
    for line in body.split("\n"):
        t = line.strip()
        if depth == 0:
            if t.startswith("#["):
                attrs.append(t)
            elif m := re.match(r"([A-Z]\w*)\s*(\{|\(|,|$)", t):
                attr = " ".join(attrs)
                rename = re.search(r'rename = "([^"]+)"', attr)
                out.append({
                    "json": rename.group(1) if rename else _rename(m.group(1), rule),
                    "aliases": re.findall(r'alias = "([^"]+)"', attr),
                    "default": "#[default]" in attr,
                    "fields": [],
                })
                attrs = []
        elif depth == 1 and out:
            if t.startswith("#["):
                attrs.append(t)
            elif m := re.match(r"(?:pub )?(\w+): (.+),$", t):
                out[-1]["fields"].append({"name": m.group(1), "type": m.group(2), "attrs": " ".join(attrs)})
                attrs = []
            elif t and not t.startswith("//"):
                attrs = []
        depth += t.count("{") - t.count("}")
    return out


def rust_enums() -> dict[str, list[dict]]:
    """Les enums désérialisables du code, par chemin (`sudoku::Difficulty`) et par nom seul."""
    enums: dict[str, list[dict]] = {}
    for path in SRC.rglob("*.rs"):
        text = path.read_text()
        # Les attributs, une ligne entière chacun (pas de retour arrière possible), puis l'enum.
        for m in re.finditer(r"((?:^[ \t]*#\[[^\n]*\n)+)[ \t]*pub enum (\w+)", text, re.M):
            if "Deserialize" not in m.group(1):
                continue
            rule = re.search(r'rename_all = "([^"]+)"', m.group(1))
            variants = _variants(_enum_body(text, m.end()), rule.group(1) if rule else None)
            enums[f"{path.stem}::{m.group(2)}"] = variants
            enums.setdefault(m.group(2), variants)
    return enums


def block_choices() -> dict[str, list[tuple[str, str, list[dict]]]]:
    """Pour chaque nom de bloc (type et alias), ses champs à valeurs imposées et leurs valeurs."""
    enums = rust_enums()
    out: dict[str, list[tuple[str, str, list[dict]]]] = {}
    for block in enums["mod::Block"]:
        choices = []
        for field in block["fields"]:
            kind = re.sub(r"^(?:Option|Vec)<(.+)>$", r"\1", field["type"]).split("::")
            values = enums.get("::".join(kind[-2:])) or enums.get(kind[-1])
            if values and not any(v["fields"] for v in values):
                choices.append((field["name"], kind[-1], values))
        for name in [block["json"], *block["aliases"]]:
            out[name] = choices
    return out


def example_type(bid: str, choices: dict) -> str:
    """Le type du bloc présenté par l'exemple `bid` : celui qui porte ce nom, sinon le premier bloc
    qui n'est pas un simple texte ou titre de décor."""
    types = [b["type"] for b in json.loads((SITE / "exemples" / f"{bid}.json").read_text())["blocks"]]
    # Le même bloc sous un autre nom (type ou alias) : `block_choices` leur donne la même liste.
    same = next((t for t in types if bid in choices and choices.get(t) is choices[bid]), None)
    return same or next((t for t in types if t not in ("text", "title")), types[0])


# Enums dont les noms JSON sont en français (les alias donnent l'anglais) ; ailleurs, l'inverse.
FRENCH_FIRST = {"When", "Theme", "Cipher", "Kind", "Zone"}
# Enums dont les alias sont des synonymes (« plus », « fois ») plutôt que des traductions : le nom
# français est donné ici quand il diffère, sinon c'est le même mot qu'en anglais (« addition »).
FRENCH_NAMES = {"Operation": {"subtraction": "soustraction"}}


def _plain(word: str) -> str:
    return "".join(c for c in unicodedata.normalize("NFD", word.lower()) if unicodedata.category(c) != "Mn")


def translations(enum: str, value: dict) -> tuple[str, str]:
    """(nom français, nom anglais) d'une valeur : son nom JSON, et dans l'autre langue le premier
    alias qui en est vraiment une traduction (pas un pluriel, une variante accentuée ou « wpa2 »)."""
    name = value["json"]
    if enum in FRENCH_NAMES:
        return FRENCH_NAMES[enum].get(name, name), name

    def variant(a: str) -> bool:
        """Même mot au pluriel, au singulier ou accentué (« additions », « reseau », « césar »)."""
        x, y = sorted((_plain(a), _plain(name)), key=len)
        return y.startswith(x) and len(y) - len(x) <= 1

    other = next((a for a in value["aliases"] if re.fullmatch(r"[a-zà-ÿ_-]{2,}", a) and not variant(a)), name)
    return (name, other) if enum in FRENCH_FIRST else (other, name)


def choices_html(choices: list[tuple[str, str, list[dict]]]) -> str:
    """Un champ par ligne, puis ses valeurs dans la langue de la page : « facile • moyen (par défaut) »."""
    if not choices:
        return ""
    rows = []
    for name, enum, values in choices:
        shown = []
        for v in values:
            fr, en = translations(enum, v)
            label = (
                f"<code>{html.escape(fr)}</code>"
                if fr == en
                else f'<code class="fr">{html.escape(fr)}</code><code class="en">{html.escape(en)}</code>'
            )
            default = f' <span class="default">({both("par défaut", "default")})</span>' if v["default"] else ""
            shown.append(f"<span>{label}{default}</span>")
        rows.append(f"<dt><code>{html.escape(name)}</code></dt><dd>{' <b class=\"sep\">•</b> '.join(shown)}</dd>")
    return f'<div class="bloc-choices"><dl>{"".join(rows)}</dl></div>'



def render_blocks() -> str:
    """Une fiche par bloc : son ticket imprimé, sa description et l'exemple JSON qui l'a produit."""
    catalogue = json.loads(CATALOGUE.read_text())
    choices = block_choices()
    slug = slugify(case="lower")
    toc, content, count = [], [], 0
    for group in catalogue:
        gid = slug(group["fr"], "-")
        items = []
        content.append(f'<h2 id="{gid}">{both(group["fr"], group["en"])}</h2>')
        for b in group["blocs"]:
            count += 1
            bid = b["id"]
            badge = '<span class="claude">Claude</span>' if b.get("claude") else ""
            # Le ticket et son exemple dans chaque langue : exemples/en/ quand il existe.
            tickets, codes = [], []
            for lang, folder in (("fr", ""), ("en", "en/")):
                png = SITE / "exemples" / folder / f"{bid}.png"
                if not png.exists():
                    folder, png = "", SITE / "exemples" / f"{bid}.png"
                source = SITE / "exemples" / folder / f"{bid}.json"
                if not source.exists():
                    source = SITE / "exemples" / f"{bid}.json"
                width, height = png_size(png)
                example = json.dumps(json.loads(source.read_text()), ensure_ascii=False, indent=2)
                tickets.append(f'<a class="bloc-ticket {lang}" href="exemples/{folder}{bid}.png"><img src="exemples/{folder}{bid}.png" '
                               f'alt="{html.escape(b[lang])}" width="{width}" height="{height}" loading="lazy"></a>')
                codes.append(f'<code class="{lang}">{html.escape(example)}</code>')
            items.append(f'<li><a href="#{bid}">{b["emoji"]} {both(b["fr"], b["en"])}</a></li>')
            content.append(f"""<section class="bloc">
  <div class="bloc-text">
    <h3 id="{bid}"><span class="emoji">{b["emoji"]}</span> {both(b["fr"], b["en"])}{badge}<a class="headerlink" href="#{bid}">#</a></h3>
    <p>{both(b["description"]["fr"], b["description"]["en"])}</p>
    <div class="bloc-spec">
      <div class="bloc-json"><pre>{"".join(codes)}</pre></div>
      {choices_html(choices.get(example_type(bid, choices), []))}
    </div>
  </div>
  {"".join(tickets)}
</section>""")
        toc.append(f'<li><a href="#{gid}">{both(group["fr"], group["en"])}</a><ul>{"".join(items)}</ul></li>')
    return BLOCKS_TEMPLATE.format(
        count=count, nav=nav_links_both(BLOCKS_PAGE), toc=f"<ul>{''.join(toc)}</ul>", content="\n".join(content),
        repo=REPO, branch=BRANCH,
    )


def main() -> None:
    shutil.rmtree(OUT, ignore_errors=True)
    shutil.copytree(SITE, OUT, ignore=SKIP)
    for page, lang, title, source, twin in PAGES:
        (OUT / page).write_text(render(page, lang, title, source, twin))
    (OUT / BLOCKS_PAGE).write_text(render_blocks())
    (OUT / ".nojekyll").touch()  # fichiers servis tels quels
    # Installation en une commande : curl -fsSL https://xninety9.github.io/Printr/install.sh | bash
    shutil.copy(ROOT / "deploy" / "get.sh", OUT / "install.sh")
    pages = sorted(p.name for p in OUT.glob("*.html"))
    print(f"site construit dans {OUT.relative_to(ROOT)} : " + ", ".join(pages))


if __name__ == "__main__":
    main()
