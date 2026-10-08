"""Construit le site dans _site/ : la page d'accueil (docs/) et les pages de documentation,
rendues depuis le README et le guide de contribution du dépôt pour ne jamais s'en écarter.

    uvx --with markdown --with pymdown-extensions python docs/build.py && python3 -m http.server -d _site
"""

import html
import os
import re
import shutil
from pathlib import Path

import markdown
from pymdownx.slugs import slugify

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "docs"
OUT = ROOT / "_site"
REPO = "https://github.com/XNinety9/Printr"
BRANCH = "master"

# (page produite, titre, source, libellé dans la navigation)
PAGES = [
    ("guide.html", "Guide", ROOT / "README.md", "Guide"),
    ("contribuer.html", "Contribuer", ROOT / "CONTRIBUTING.md", "Contribuer"),
]
# Fichiers du dépôt qui ont leur page sur le site.
PAGE_FOR = {"README.md": "guide.html", "CONTRIBUTING.md": "contribuer.html"}
# Ce qui, dans docs/, ne sert qu'à fabriquer le site ou ses illustrations.
SKIP = shutil.ignore_patterns("build.py", "demo", "*.sh", "*.py", "__pycache__")

TEMPLATE = """<!DOCTYPE html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Printr</title>
<meta name="description" content="Documentation de Printr : {title_lower}.">
<link rel="icon" href="mark.png" type="image/png">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500;700&display=swap" rel="stylesheet">
<script>
  // Thème choisi sur la page d'accueil, avant le premier affichage.
  try {{ var t = localStorage.getItem("printr-theme"); if (t === "dark" || t === "light") document.documentElement.setAttribute("data-theme", t); }} catch (e) {{}}
</script>
<link rel="stylesheet" href="docs.css">
</head>
<body>
<nav class="nav">
  <div class="wrap">
    <a class="brand" href="./"><img src="mark.png" alt="">Printr</a>
    <div class="links">{nav}<a href="{repo}">GitHub</a></div>
  </div>
</nav>
<main class="wrap docs">
  <aside class="toc"><div class="title">Sur cette page</div>{toc}</aside>
  <article class="doc">
    <h1>{title}</h1>
    {content}
    <p class="caption">Page générée depuis <a href="{repo}/blob/{branch}/{source}">{source}</a>.</p>
  </article>
</main>
<script src="toc.js" defer></script>
</body>
</html>
"""


def readme_body(text: str) -> str:
    """Le README sans son logo, sa devise, ses badges et ses liens d'en-tête : le site les a."""
    return re.sub(r"\A(?:\s*<p align=\"center\">.*?</p>)+\s*", "", text, flags=re.S)


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


def render(page: str, title: str, source: Path) -> str:
    text = source.read_text()
    text = readme_body(text) if source.name == "README.md" else re.sub(r"^# .*\n", "", text, count=1)
    md = markdown.Markdown(
        # superfences : blocs de code dans les listes ; md_in_html : Markdown dans le HTML du README.
        extensions=["pymdownx.superfences", "tables", "toc", "sane_lists", "attr_list", "md_in_html"],
        extension_configs={
            # Ancres à la GitHub (accents gardés) : les liens internes du README restent valides.
            "toc": {"permalink": "#", "toc_depth": "2-3", "slugify": slugify(case="lower")},
        },
    )
    body = rewrite_links(md.convert(text), source)
    nav = "".join(
        f'<a href="{p}"{" aria-current=\"page\"" if p == page else ""}>{label}</a>' for p, _, _, label in PAGES
    )
    return TEMPLATE.format(
        title=title,
        title_lower=title.lower(),
        nav=nav,
        # Le sommaire de Python-Markdown a son propre <div class="toc"> : la page fournit le sien.
        toc=re.sub(r'\A\s*<div class="toc">\s*|\s*</div>\s*\Z', "", md.toc),
        content=body,
        repo=REPO,
        branch=BRANCH,
        source=source.relative_to(ROOT).as_posix(),
    )


def main() -> None:
    shutil.rmtree(OUT, ignore_errors=True)
    shutil.copytree(SITE, OUT, ignore=SKIP)
    for page, title, source, _ in PAGES:
        (OUT / page).write_text(render(page, title, source))
    (OUT / ".nojekyll").touch()  # fichiers servis tels quels
    pages = sorted(p.name for p in OUT.glob("*.html"))
    print(f"site construit dans {OUT.relative_to(ROOT)} : " + ", ".join(pages))


if __name__ == "__main__":
    main()
