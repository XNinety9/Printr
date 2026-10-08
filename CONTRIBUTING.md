# Contribuer à Printr

Merci de ton intérêt ! Ce guide explique comment mettre le projet en route, comment il est
organisé et comment proposer une modification. Pas besoin d'imprimante : un émulateur et un
aperçu dans le terminal suffisent pour presque tout.

## Mettre en route

Il faut une chaîne Rust stable (`rustup`). Pour l'émulateur et les captures, il faut aussi
[`uv`](https://docs.astral.sh/uv/), et Chromium pour les captures de l'interface.

```sh
git clone https://github.com/XNinety9/Printr && cd Printr
cargo test                                               # tous les tests, sans réseau ni imprimante
cargo run -- --preview print examples/complet.json       # aperçu du ticket complet dans le terminal
```

Quelques variables d'environnement utiles :

| Variable | Rôle |
| --- | --- |
| `ANTHROPIC_API_KEY` | Blocs Claude (`horoscope`, `word_of_the_day`). Facultative : sans elle, ces blocs affichent une erreur et le reste du ticket sort normalement. |
| `PRINTR_BARNUM` | Commande de [Barnum](https://github.com/XNinety9/Barnum) pour le bloc `barnum`, par exemple `python3 ../Barnum/main.py`. |
| `PRINTR_DATA_DIR` | Comptes, presets et historique de l'interface web. Utilise un dossier jetable pour tes essais. |
| `PRINTR_GLITCH` | Fréquence des glitchs (une impression sur N, 20 par défaut, 0 : jamais). |
| `PRINTR_CACHE_DIR` | Cache des contenus du jour. |
| `PRINTR_WEB_DIR=web` | Sert l'interface depuis le disque : on modifie `web/` sans recompiler. |
| `PRINTR_DEBUG=1` | Affiche les réponses brutes de Claude et le classement des actualités. |

## Tester sans imprimante

- **Aperçu terminal** : `--preview` affiche le ticket encadré, avec gras, inversé et soulignés.
- **Émulateur** : `scripts/demo.sh [ticket.json]` imprime dans l'émulateur
  [emupos](https://pypi.org/project/emupos/) (profil TM-T88V dans `emulator/`) et donne le PNG
  du ticket, au point près.
- **Octets bruts** : `--dump sortie.bin` écrit exactement ce qui partirait vers l'imprimante.
- **Exemples de blocs** : `docs/exemples/generer.sh [bloc…]` réimprime les exemples de
  `docs/exemples/` dans l'émulateur, avec des données fictives. Pense à y ajouter ton bloc.
- **Interface web** :

  ```sh
  export PRINTR_DATA_DIR=/tmp/printr-dev PRINTR_WEB_DIR=web
  echo motdepasse | cargo run -- user add Test
  cargo run -- --dump /tmp/sortie.bin serve --listen 127.0.0.1:8080
  ```

Les blocs Claude coûtent de l'argent à chaque génération. Pour itérer, préfère l'aperçu de
l'interface web, qui n'appelle jamais Claude, ou le cache du jour : sans `--refresh`, un
horoscope déjà généré aujourd'hui est repris tel quel.

## Organisation du code

| Fichier | Rôle |
| --- | --- |
| `src/main.rs` | Ligne de commande (clap), aide en français |
| `src/blocks/mod.rs` | Le format JSON des tickets, l'enum `Block`, la construction en parallèle |
| `src/blocks/*.rs` | Un fichier par bloc un peu fourni (météo, sudoku, Barnum…) |
| `src/doc.rs` | Représentation intermédiaire du ticket : lignes stylées, images, QR codes, aperçus |
| `src/cp858.rs` | Encodage du texte pour l'imprimante (page de code PC858) |
| `src/raster.rs`, `src/draw.rs` | Images : tramage, dessins (sudoku, labyrinthe, QR codes côte à côte) |
| `src/output.rs` | Envoi vers l'imprimante USB, TCP ou un fichier |
| `src/server.rs`, `src/store.rs`, `src/scheduler.rs` | Serveur web, données (comptes, presets, historique), planificateur |
| `src/claude.rs`, `src/cache.rs` | Client de l'API Claude, cache des contenus du jour |
| `src/ui.rs` | Sortie console : progression, résumé, erreurs, journal du serveur |
| `web/` | Interface web (Preact + htm, sans compilation), intégrée au binaire |
| `docs/` | Site vitrine (GitHub Pages) et ses illustrations |

## Développer un nouveau bloc

Un bloc, c'est une entrée du JSON d'un ticket (`{ "type": "…", … }`) qui se transforme en un
morceau de ticket. Pour le développer, on suit toujours le même chemin, illustré ici avec un
bloc d'exemple, « Dés », qui lance quelques dés et affiche leur total :

```json
{ "type": "dice", "count": 3 }
```

### 1. Déclarer le bloc et ses paramètres

Dans `src/blocks/mod.rs`, ajoute une variante à l'enum `Block`. Son nom, en `snake_case`, donne
le `type` du JSON ; ses champs sont les paramètres.

```rust
    /// Lancer de dés.
    Dice {
        /// Nombre de dés, de 1 à 6.
        #[serde(default = "two")]
        count: u8,
    },
```

- Donne une valeur par défaut (`#[serde(default)]`, ou une fonction comme `two`) à tout ce qui
  n'est pas indispensable : un ticket minimal doit fonctionner.
- Un paramètre inconnu est refusé (`deny_unknown_fields`), ce qui signale les fautes de frappe.
- Pour une liste de choix, déclare un `enum` avec `#[serde(rename_all = "snake_case")]` et des
  alias français (`#[serde(alias = "facile")]`), comme `sudoku::Difficulty`.

### 2. Écrire le rendu

Crée `src/blocks/dice.rs`. Sa fonction `build` reçoit les paramètres et renvoie un `Doc`, la
représentation du ticket avant impression :

```rust
//! Lancer de dés : un ou plusieurs dés à six faces, et leur total.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::doc::{Doc, Style};

/// Tire `count` dés (de 1 à 6 dés).
fn roll(count: u8, rng: &mut StdRng) -> Vec<u8> {
    (0..count.clamp(1, 6)).map(|_| rng.random_range(1..=6)).collect()
}

pub fn build(count: u8) -> Doc {
    let rolls = roll(count, &mut StdRng::from_rng(&mut rand::rng()));
    let faces: Vec<String> = rolls.iter().map(u8::to_string).collect();
    let total: u32 = rolls.iter().map(|&r| u32::from(r)).sum();

    let mut doc = Doc::new();
    doc.header("Lancer de dés");
    doc.feed(1);
    doc.text(&faces.join("  "), Style::default().bold().center().size(2));
    doc.text(&format!("Total : {total}"), Style::default().center());
    doc
}
```

La boîte à outils de `Doc` (`src/doc.rs`) :

| Méthode | Effet |
| --- | --- |
| `header("Titre")` | Bandeau inversé (blanc sur noir) sur toute la largeur |
| `text(texte, style)` | Texte coupé aux mots ; une ligne vide saute une ligne |
| `hanging("Préfixe : ", texte, style)` | Texte dont les lignes suivantes s'alignent après le préfixe |
| `line(texte, style)` | Une ligne telle quelle, espaces conservés : idéal pour des colonnes |
| `rule('─')`, `feed(n)` | Ligne de séparation, lignes vides |
| `image(img)`, `qr(données, taille)` | Image de 512 points de large, QR code |

Et pour `Style` : `bold()`, `underline()`, `reverse()`, `small()` (police B), `size(1..=8)`,
`center()`, `align(…)`, `upside_down()`. Le papier fait 42 colonnes en police normale, 56 en
petite police, et 42 divisé par la taille quand on agrandit (21 colonnes en taille 2).

Pour une image (dessin, grille, photo), voir `draw.rs` et `raster.rs` : `draw::canvas(hauteur)`
donne un canevas blanc de 512 points de large, `draw::fill_rect` y dessine, et
`raster::prepare` trame une photo.

### 3. Brancher le bloc

Toujours dans `src/blocks/mod.rs`, trois ajouts :

```rust
mod dice;                                          // en haut du fichier

Block::Dice { .. } => "dés",                       // dans Block::name()

Block::Dice { count } => doc = dice::build(*count), // dans Block::build()
```

Si un paramètre précise utilement le bloc (une ville, un signe…), ajoute-le aussi dans
`Block::label()` : il apparaît dans la console (« météo · Lyon »), l'historique et les erreurs.

### 4. Essayer

```sh
echo '{"blocks":[{"type":"dice","count":3}]}' | cargo run -- --preview print
```

```
╭──────────────────────────────────────────────────────────╮
│                      LANCER DE DÉS                       │
│                                                          │
│                        1   4   2                         │
│                        Total : 7                         │
╰╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ ✂ ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╯
```

Puis le vrai rendu, au point près, dans l'émulateur :

```sh
echo '{"blocks":[{"type":"dice","count":3}]}' > /tmp/des.json && scripts/demo.sh /tmp/des.json
```

### 5. Tester

Ajoute des tests en bas du fichier. Isole ce qui est aléatoire ou réseau pour pouvoir le tester
de façon déterministe, ici avec une graine fixe :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls_are_valid_dice() {
        let mut rng = StdRng::seed_from_u64(42);
        for count in [0, 1, 3, 6, 9] {
            let rolls = roll(count, &mut rng);
            assert_eq!(rolls.len(), count.clamp(1, 6) as usize);
            assert!(rolls.iter().all(|r| (1..=6).contains(r)));
        }
    }
}
```

### 6. L'ajouter à l'interface web

Dans `web/app.js`, ajoute une entrée au catalogue `BLOCKS`. Les champs du formulaire sont
générés à partir de `fields` :

```js
  {
    type: 'dice', label: 'Dés', emoji: '🎲', group: G.fun, desc: 'Un lancer de dés et son total',
    defaults: { count: 2 },
    fields: [{ key: 'count', label: 'Nombre de dés', kind: 'number', min: 1, max: 6 }],
    summary: (b) => `${b.count} dé${b.count > 1 ? 's' : ''}`,
  },
```

Types de champs disponibles (`kind`) : `text`, `textarea`, `number`, `select`, `segmented`
(boutons côte à côte), `toggle`, `date`, `list` (liste de textes), `icons` (choix par emoji) et
`image` (photo envoyée depuis le téléphone). Ajoute `ai: true` si le bloc appelle Claude : il
reçoit alors le badge « Claude » dans le catalogue. Un paramètre sans champ dans le formulaire
est conservé tel quel à l'enregistrement.

Avec `PRINTR_WEB_DIR=web`, recharge simplement la page pour voir ton bloc dans le catalogue.

### 7. Le documenter

- un exemple dans `docs/exemples/` (`mon_bloc.json`), dont `generer.sh mon_bloc` tire le PNG ;
- une ligne dans le tableau des blocs du `README.md` (paramètres et valeurs par défaut) ;
- une puce dans le catalogue de `docs/index.html`, en français et en anglais.

### Cas particuliers

**Un bloc qui interroge une API** reçoit le contexte (`ctx: &Ctx`) et utilise son client HTTP,
qui a déjà un délai d'expiration :

```rust
let data: Reponse = ctx.http.get("https://…").query("ville", ville).call()?.body_mut().read_json()?;
```

Préfère les API gratuites et sans clé, et gère leur panne par une erreur claire (`anyhow`). Pour
le tester sans réseau, sépare le décodage et le rendu de l'appel, et teste-les sur une réponse
d'exemple (voir `barnum.rs`).

**Un bloc qui appelle Claude** (voir `horoscope.rs` et `word.rs`) :

- demande une réponse JSON structurée avec `ctx.claude.generate(système, prompt, schéma, effort)` ;
- en aperçu (`ctx.preview`), n'appelle jamais Claude : renvoie `ai_placeholder(titre, description)` ;
- garde le résultat en cache pour la journée (`ctx.cache`, avec `get` et `put`, sous une clé qui
  contient la date et `PROMPT_VERSION`), et ignore le cache si `ctx.refresh` est vrai ;
- vérifie que la réponse est complète, et retente une fois si un champ revient vide.

**Les règles qui valent pour tous les blocs** :

- un bloc en échec ne fait jamais tomber le ticket : renvoie une erreur, le ticket imprimera un
  message à sa place ;
- pas d'emoji ni de caractères exotiques à imprimer, que la page PC858 ne contient pas : pour
  un dessin, passe par une image (voir `picto.rs`) ;
- les blocs se construisent en parallèle : un bloc lent ne retarde pas les autres, mais le
  ticket attend le plus lent avant d'imprimer.

## Conventions

- **Langue** : le code est en anglais ; les commentaires, messages, textes imprimés et la
  documentation sont en français, avec la typographie française (espace avant « : ; ! ? »,
  guillemets « »).
- **Style** : `cargo fmt` et `cargo clippy` sans avertissement. Le build ne doit produire aucun
  warning.
- **Sortie console** : passe par `ui.rs` et `anstream` (couleurs coupées automatiquement hors
  terminal ou avec `NO_COLOR`).
- **Commits** : des messages en français, à l'impératif ou au nominal (« Blocs énigme et défi
  sportif »), avec un paragraphe d'explication quand le changement n'est pas évident.

## Interface web et site

- L'interface vit dans `web/`. Avec `PRINTR_WEB_DIR=web`, un simple rechargement de la page
  suffit. Vérifie-la sur téléphone (390 px de large) comme sur ordinateur, en clair et en
  sombre.
- Les captures du site et du README se régénèrent avec `docs/demo/photo-session.sh`, toujours
  sur des données fictives. N'y mets jamais de vraies données personnelles.
- Les déclinaisons du logo se régénèrent avec
  `uv run --with pillow python docs/brand/make-assets.py`.

## Proposer une modification

1. Crée une branche à partir de `master`.
2. Vérifie que `cargo test` passe et que `cargo build` ne produit aucun warning.
3. Si tu touches au rendu, joins une capture de l'aperçu ou de l'émulateur à ta demande de
   fusion.
4. Ouvre une pull request en décrivant le pourquoi autant que le comment.

Une question, une idée de bloc ? Ouvre une issue.
