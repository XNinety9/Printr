<p align="center"><a href="guide.md">🇬🇧 English version</a></p>

# Guide de Printr

Tout sur Printr : les blocs, l'appli web, l'API HTTP, MQTT, l'installation et le développement.
Pour un aperçu rapide, voir le [README](../README.fr.md).

## Démarrage rapide

```sh
cargo run --release -- --preview print examples/complet.json   # aperçu dans le terminal, sans imprimante
cargo run --release -- --device /dev/usb/lp0 serve             # interface sur http://localhost:8080
scripts/deploy.sh pi@printer.local                             # installation sur un Raspberry Pi (ou --local)
```

Au premier lancement, l'interface propose de créer son compte, puis un court
[assistant](#premier-lancement) règle l'imprimante, la ville et Claude ; viennent ensuite les comptes
de la famille.

**Le plus simple, sur un Raspberry Pi** : écrire l'[image prête à flasher](#image-prête-à-flasher)
sur une carte microSD, brancher l'imprimante et allumer. Même le Wi-Fi se règle depuis le téléphone.

Pas encore d'imprimante ? `scripts/demo.sh` imprime le ticket complet dans un
[émulateur](#tester-sans-imprimante-émulateur) et en donne une image PNG.

## L'horoscope, en quatre tons

Rédigé chaque jour par Claude, sur un thème tiré au hasard. Extraits authentiques :

| Ton | Signe | Extrait |
| --- | --- | --- |
| `serieux` | Scorpion | « Une conversation franche dissipera un malentendu qui pesait depuis plusieurs jours. » |
| `farfelu` | Gémeaux | « Un pigeon particulièrement élégant pourrait vous saluer d'un hochement de tête : rendez-lui la politesse, il est très influent dans le quartier. » |
| `vachard` | Vierge | « Votre partenaire rêve d'un dîner romantique. Rien de plus sensuel qu'une balance de cuisine posée entre deux bougies. » |
| `insultant` | Lion | « Tes baskets neuves, encore raides comme un notaire, se demandent ce qu'elles ont fait au bon Dieu pour tomber sur une feignasse pareille. » |

## Utilisation

```sh
printr print examples/matin.json    # ticket composé de blocs (voir plus bas)
cat ticket.json | printr print      # idem, depuis l'entrée standard
printr --preview print ticket.json  # aperçu dans le terminal, sans imprimer ni appeler Claude (--claude pour l'autoriser)
printr --refresh print ticket.json  # régénère ce qui est en cache : soleil, horoscopes, mot du jour
printr -q print ticket.json         # silencieux, n'affiche que les erreurs
printr test                         # ticket de test (styles, accents, QR code)
printr text "Salut !"               # texte libre, puis coupe
echo "depuis stdin" | printr text   # lit l'entrée standard
printr text --no-cut "sans coupe"
printr image photo.jpg             # image réduite à 512 px, tramée (Floyd–Steinberg)
printr image --dither atkinson photo.jpg  # autres tramages : atkinson (plus contrasté), bayer (trame régulière)
printr image --no-dither logo.png   # simple seuil noir/blanc, pour logos et dessins au trait
```

L'imprimante ne fait que des points noirs : le tramage traduit les gris d'une photo en motifs de points.
La même photo avec chaque algorithme (`--dither`, ou `dither_algorithm` dans un ticket JSON, aussi dans
l'appli) :

<p align="center"><img src="exemples/tramages.png" alt="La même photo de café imprimée avec Floyd–Steinberg, Atkinson, Bayer et sans tramage" width="256"></p>

Floyd–Steinberg convient à la plupart des photos ; Atkinson donne plus de contraste et des aplats plus
nets, Bayer une trame régulière sans grain ; sans tramage, c'est pour les logos et les dessins au trait.

Destination, au choix :

| Option | Destination |
| --- | --- |
| `--device <chemin>` | imprimante USB (défaut `/dev/usb/lp0`, ou `$PRINTR_DEVICE`) |
| `--tcp hôte:port` | imprimante réseau ou émulateur |
| `--dump <fichier>` | écrit les octets ESC/POS bruts dans un fichier |

## Tickets JSON

Un ticket est une liste de blocs. Les blocs sont construits en parallèle (requêtes réseau
comprises) et leur résultat s'affiche dans la CLI dès que chacun est prêt ; le ticket est ensuite
imprimé dans l'ordre défini. Un bloc en échec imprime un message au lieu de bloquer le ticket.
Un paramètre inconnu est une erreur, pour repérer les fautes de frappe.

```json
{
  "cut": true,
  "spacing": 1,
  "blocks": [
    { "type": "title", "text": "Bonjour !" },
    { "type": "weather", "location": "Lyon", "days": 3 },
    { "type": "horoscope", "sign": "scorpion", "tone": "farfelu" }
  ]
}
```

| Bloc | Paramètres (défaut) | Notes |
| --- | --- | --- |
| `title` | `text`, `size` (2) | Gras, centré, agrandi |
| `text` | `text`, `bold`, `underline`, `reverse`, `small`, `size` (1), `align` (`left`/`center`/`right`) | Coupé aux mots |
| `separator` | `style` (`-`) | Un caractère répété : `-`, `=`, `═`, `─`, `·`… |
| `date` | `offset` (jours, par ex. 1 pour demain), `date` (`AAAA-MM-JJ`) | « Mercredi 7 octobre 2026 » ; aujourd'hui par défaut, ou un autre jour, pour préparer des tickets à l'avance |
| `feed` | `lines` (1) | Espace vertical |
| `cut` (ou `coupe`) | — | Coupe le papier à cet endroit, après l'espacement habituel (`spacing`) : la suite sort sur un autre ticket. Le `cut` du ticket ajoute toujours la coupe finale |
| `image` | `path` ou `url`, `dither` (true), `dither_algorithm` (`floyd-steinberg`, `atkinson`, `bayer`) | Réduite à 512 px |
| `qr` | `data`, `size` (6), `caption` | |
| `weather` | `location`, `days` (1, max 7), `outfit` (ou `tenue`, false), `wind` (true), `sun` (true) | [Open-Meteo](https://open-meteo.com), sans clé. `location` : ville ou `"lat,lon"`. Pictogrammes pour chaque jour ; `outfit` ajoute comment s'habiller aujourd'hui (« manteau et bonnet, prends un parapluie »), sans IA ; `wind` et `sun` affichent la ligne du vent et de l'UV, et le lever et le coucher du soleil. °F et mph avec `PRINTR_UNITS=imperial` (dans les réglages) |
| `barnum` | `sign` ou `birth_date` (`AAAA-MM-JJ`), `sky` (false), `variant` | Horoscope hors ligne et gratuit, calculé sur le vrai ciel par [Barnum](https://github.com/XNinety9/Barnum) : jauges par domaine, conseil, et en option la position des planètes |
| `horoscope` | `sign`, `tone` (`serieux`/`farfelu`/`vachard`/`insultant`) | Généré par Claude, sur un thème tiré au hasard. Signe en français ou en anglais |
| `saint` | — | Calendrier local, hors ligne |
| `todo` | `items`, `title` (« À faire ») | Cases à cocher |
| `recipe` (ou `recette`) | `url`, `steps` (true), `qr` (true) | Recette lue sur son site (données schema.org, présentes sur la plupart des sites de cuisine) : ingrédients à cocher et étapes numérotées, sans la publicité ; `"steps": false` n'imprime que les ingrédients |
| `recipe_claude` (ou `recette_claude`) | `dish` (ou `plat`), `servings` (4), `steps` (true) | Recette d'un plat rédigée par Claude à l'impression, même présentation ; gardée en cache par plat |
| `recipe_from_ingredients` (ou `recette_ingredients`) | `ingredients` (un par ligne ou séparés par des virgules), `servings` (4), `wish` (ou `envie`, facultatif : « rapide », « végétarien »…), `steps` (true) | Qu'est-ce qu'on mange ? Claude rédige une recette avec les ingrédients qu'on a, plus seulement les basiques du placard |
| `sudoku` | `difficulty` (`facile`/`moyen`/`difficile`), `seed`, `solution` | Le n° imprimé est la graine : même `seed` + `"solution": true` imprime la solution |
| `word_search` (ou `mots_meles`, `mots_caches`, `mots_en_grille`) | `difficulty` (`facile`/`moyen`/`difficile`), `theme`, `words`, `seed`, `solution` | Mots mêlés en français. `theme` : `animaux`, `fruits_legumes`, `cuisine`, `nature`, `sport`, `metiers`, `maison`, `voyage`, `musique`, `ecole`, `developpement`, `devops`, `reseaux`, `ia` (tiré du n° si absent) ; ou ses propres mots dans `words`. Facile : 10×10, 8 mots, → ↓ ; moyen : 12×12, 12 mots, + diagonales ; difficile : 14×14, 16 mots, dans tous les sens. Même n° + `"solution": true` imprime la solution |
| `train_tracks` (ou `voie_ferree`, `rails`) | `difficulty` (`facile`/`moyen`/`difficile`), `seed`, `solution` | Relier A (bord gauche) à B (bord bas) par une seule voie, d'après le nombre de cases de voie de chaque ligne et colonne et quelques pièces posées. Grilles de 6×6, 8×8 et 10×10, à solution unique. Même `seed` + `"solution": true` imprime la solution |
| `arrow_word` (ou `mots_fleches`) | `level` (1 à 4, défaut 2), `size` (`petit`/`moyen`/`grand` : 7×9, 9×11, 11×13), `seed`, `solution` | Mots fléchés générés par [CrossWords](https://github.com/XNinety9/CrossWords) : grille à numéros et définitions en dessous, rédigées par Claude à l'impression. Même n°, taille et niveau + `"solution": true` imprime la solution |
| `crossword` (ou `mots_croises`) | `level` (1 à 4, défaut 2), `size` (`petit`/`moyen`/`grand` : 7×7, 9×9, 11×11), `seed`, `solution` | Mots croisés générés par [CrossWords](https://github.com/XNinety9/CrossWords) : rangées en chiffres romains et colonnes en chiffres, définitions « Horizontalement » et « Verticalement » rédigées par Claude à l'impression (cases numérotées et « Across / Down » en anglais). Même numéro, même taille et même niveau + `"solution": true` imprime la solution |
| `maze` | `width` (12), `height` (16), `seed` | |
| `word_of_the_day` | — | Choisi par Claude, sans répéter les 60 derniers mots |
| `quote` | — | Citation du jour, liste locale |
| `fortune` | `cow` (ou `vache`, false), `source` (`builtin`, ou `system` pour la commande `fortune`), `seed` | Un petit papier façon biscuit chinois, hors ligne ; `cow` le fait dire par la vache de cowsay. `system` demande `fortune-mod` sur le serveur |
| `holidays` | `zone` (`metropole`/`alsace-moselle`), `count` (1) | Calcul local, vérifié contre calendrier.api.gouv.fr |
| `countdown` | `label`, `date` (`AAAA-MM-JJ`) | « J-79 avant : Noël » |
| `birthdays` (ou `anniversaires`) | `list` (une ligne par personne : `Léo, 12/03/2014`, année facultative), `upcoming` (3), `family` (true) | « Joyeux anniversaire Léo ! » avec l'âge le jour J, puis le compte à rebours des suivants. Jour d'abord en français, mois d'abord en anglais, date ISO partout. `family` ajoute la liste commune de la famille, que chacun remplit depuis l'appli (`GET`/`PUT /api/birthdays`) |
| `moon` | — | Phase, illumination, prochaines pleine et nouvelle lunes. Calcul local |
| `on_this_day` | `count` (3) | Éphéméride, Wikipédia en français |
| `air_quality` | `location`, `pollutants` (true), `pollen` (true) | Indice européen, particules, pollens (Open-Meteo) ; `pollutants` et `pollen` affichent leurs lignes |
| `crypto` | `coins` (`["bitcoin", "ethereum"]`, identifiants CoinGecko), `currency` (`eur`) | Cours et variation sur 24 h |
| `sun` | `location` | Lever, coucher et durée du jour via Open-Meteo, sans clé |
| `sky` (ou `ciel`) | `location` | Le ciel ce soir : planètes visibles (direction et hauteur), passages visibles de la Station spatiale internationale jusqu'à l'aube, lune et étoiles filantes. Calculé sur place (éléments orbitaux du JPL, SGP4) ; seule l'orbite de l'ISS vient de CelesTrak, une fois par jour |
| `riddle` (ou `enigme`) | `kind` (`devinette`/`charade`/`logique`/`calcul`), `number`, `answer` (`envers`/`lendemain`/`dessous`/`aucune`) | 125 énigmes, une par jour sans répétition. Par défaut, la réponse est imprimée à l'envers : on retourne le ticket pour la lire |
| `workout` (ou `defi_sportif`) | `level` (`facile`/`moyen`/`difficile`), `number` | Défi sportif du jour, sans équipement, 25 défis par niveau |
| `news` | `title`, `feeds` (URL RSS/Atom), `count` (3, max 5), `qr` (2, max 2), `themes`, `exclude`, `max_age_hours` (24) | Revue de presse sans IA : voir ci-dessous |
| `petit_bac` | `players` (1), `letter`, `count` (6), `categories`, `seed` | Une lettre et des catégories, une feuille par joueur, toutes avec la même lettre |
| `mental_math` (ou `calcul_mental`) | `difficulty` (`debutant`/`facile`/`moyen`/`difficile`), `operations` (`addition`, `subtraction`, `multiplication`, `division`, ou `+ - × ÷` ; toutes par défaut), `tables` (tables à réviser, ex. `[2, 5]`), `count` (10), `seed` | Fiche d'opérations, résultats imprimés à l'envers ; « débutant » : sommes jusqu'à 10, soustractions sans résultat négatif |
| `anagram` (ou `mot_mystere`) | `theme` (comme `word_search`), `count` (3), `seed` | Lettres mélangées, thème et longueur en indice, réponses à l'envers |
| `nonogram` (ou `logimage`) | `number` (1 à 10), `solution` | Logimage de 10×10 ; chaque dessin a une solution unique, trouvable par déduction (vérifié par les tests) |
| `kakuro` | `difficulty` (`facile`/`moyen`/`difficile`), `seed`, `solution` | Kakuro de 6×6, 8×8 ou 9×9 cases, à solution unique (quelques chiffres donnés quand les sommes seules en permettent plusieurs) ; même système de numéro que le sudoku |
| `takuzu` (ou `binairo`) | `difficulty`, `seed`, `solution` | Takuzu de 6×6, 8×8 ou 10×10 cases, à solution unique |
| `hashi` (ou `ponts`) | `difficulty`, `seed`, `solution` | Hashi (ponts) sur une grille de 7×7, 9×9 ou 11×11, de 9 à 22 îles, à solution unique |
| `cipher` (ou `message_code`) | `message`, `cipher` (`cesar`/`morse`/`nombres`), `shift`, `answer` (true) | Message codé et sa grille de déchiffrement ; message au hasard si absent |
| `chess` (ou `echecs`) | `solution` (true) | Problème du jour de Lichess : l'échiquier du côté qui joue, le dernier coup encadré, la solution en notation algébrique, à l'envers |
| `coloring` (ou `coloriage`) | `seed` | Mandala à colorier, différent à chaque numéro |
| `coupon` (ou `bon`) | `text`, `from`, `to`, `valid_until`, `count` (1) | « Bon pour… » avec frise, numéro de série et ligne de découpe ; idée au hasard si `text` est absent |
| `story` (ou `histoire`) | `children` (ou `enfants`, `"Léo, Camille"`), `theme` (facultatif), `length` (`short`/`long`, ou `courte`/`longue`) | Histoire du soir inventée par Claude, avec les enfants pour héros ; une nouvelle chaque jour, jamais deux fois le même titre |
| `shopping` (ou `courses`) | `title`, `clear` (false) | La liste de courses partagée de l'appli ; avec `clear`, le serveur la vide une fois imprimée |
| `agenda` | `calendars` (liens ICS ou `webcal://`, ou fichiers), `days` (1, max 7), `title` | Rendez-vous du jour, répétitions et fuseaux horaires compris |
| `bins` (ou `poubelles`) | `collections` (`name`, `days`, `every`, `from`), `when` (`veille`/`jour`), `always` | « Ce soir, on sort le bac jaune » ; rien n'est imprimé les soirs sans ramassage, sauf avec `always` |
| `chores` (ou `taches`) | `tasks`, `people`, `period` (`week`/`day`, ou `semaine`/`jour`) | Qui fait quoi : les tâches tournent entre les personnes chaque semaine (ou chaque jour), d'après la date |
| `wifi` | `ssid`, `password`, `security` (`wpa`/`wep`/`none`), `hidden`, `show_password` (true) | QR code qui connecte directement au réseau |
| `monthly_report` (ou `bilan`) | `month` (`AAAA-MM`, mois en cours par défaut) | Bilan façon ticket de caisse, tiré de l'historique de l'appli |

Chaque bloc a son exemple dans [`docs/exemples/`](exemples/) : le ticket JSON et son rendu,
régénérés par `docs/exemples/generer.sh` (données fictives). Ils sont tous présentés, en images,
sur la [page des blocs](https://xninety9.github.io/Printr/blocs.html) du site.

<p align="center">
  <img src="exemples/logimage.png" alt="Logimage" width="200">
  &nbsp;
  <img src="exemples/coloriage.png" alt="Mandala à colorier" width="200">
  &nbsp;
  <img src="exemples/bilan.png" alt="Bilan du mois façon ticket de caisse" width="200">
</p>

Exemple de ramassage, avec le bac jaune un vendredi sur deux :

```json
{ "type": "poubelles", "collections": [
  { "name": "ordures ménagères", "days": ["lundi", "vendredi"] },
  { "name": "bac jaune", "days": ["vendredi"], "every": 2, "from": "2026-10-09" }
] }
```

### Actualités (`news`)

Le bloc lit les flux RSS ou Atom fournis, sans IA, gratuitement et en moins d'une seconde.
Les articles sont classés selon :

- les **thèmes** demandés : trouvés dans le titre (le plus fort), la rubrique de l'URL ou les
  catégories, ou le chapô. Un thème se cherche en début de mot (« climat » trouve « climatique ») ;
- leur **place dans le flux** (la une de la rédaction d'abord) ;
- les **recoupements** : un sujet traité par plusieurs flux remonte ;
- leur **fraîcheur** (au-delà de `max_age_hours`, l'article est écarté).

Les directs, vidéos, podcasts et tribunes sont écartés, ainsi que les articles contenant un mot
de `exclude`. Deux articles sur le même sujet ne sont jamais retenus ensemble. Les `qr` premiers
reçoivent un QR code, deux par ligne. `PRINTR_DEBUG=1` affiche le classement complet.

```json
{ "type": "news", "title": "France",
  "feeds": ["https://www.franceinfo.fr/titres.rss", "https://www.lemonde.fr/rss/une.xml"],
  "themes": ["climat", "sciences", "santé"], "exclude": ["football"] }
```

Le bloc `barnum` appelle [Barnum](https://github.com/XNinety9/Barnum) en ligne de commande :
`barnum` s'il est installé, sinon la commande donnée dans `PRINTR_BARNUM`, par exemple
`PRINTR_BARNUM="python3 /opt/barnum/main.py"` (Barnum ne dépend que de Python 3.10).

De temps en temps (une impression sur 20), la machine glisse dans le ticket un message étrange,
sans en-tête, comme du bruit imprimé : c'est le **glitch**. `PRINTR_GLITCH` règle la fréquence
(`PRINTR_GLITCH=5` pour une sur cinq, `0` pour jamais) ; `{ "type": "glitch" }` en force un.
Il n'apparaît jamais dans les aperçus.

Le bloc `arrow_word` s'appuie sur la bibliothèque [CrossWords](https://github.com/XNinety9/CrossWords)
(Apache 2.0, © 2026 Pierre, [XNinety9](https://github.com/XNinety9) ; voir [`NOTICE`](../NOTICE)).
Au premier usage, elle télécharge sa liste de mots avec `curl` (une trentaine de secondes, une
seule fois) dans le cache de printr.
Les définitions viennent de son cache (`crosswords/definitions.txt`, ou `definitions-en.txt` en
anglais), complété à l'impression
par Claude pour les mots inconnus : moins d'un centime par grille neuve aux niveaux 1 et 2
(Claude Haiku), une dizaine aux niveaux 3 et 4 (Claude Opus). L'aperçu n'appelle jamais Claude,
et un mot que Claude n'a pas su définir n'est pas redemandé : sa longueur est imprimée.

### Claude : clé API ou abonnement

Les blocs Claude (`horoscope`, `word_of_the_day`, `recipe_claude`, `recipe_from_ingredients`, `story`, définitions des grilles)
passent par l'un de ces deux chemins :

| | API Claude | CLI Claude Code |
|---|---|---|
| Accès | une clé `ANTHROPIC_API_KEY` | un abonnement Claude (Pro, Max…) |
| Coût | facturé à l'appel (quelques centimes par jour) | compté dans les limites de l'abonnement |
| Durée d'un appel | quelques secondes | quelques secondes de plus (lancement de la CLI) |
| Mémoire | rien de plus | environ 260 Mo pendant l'appel |
| Machines | toutes | 64 bits seulement (`aarch64`, `x86_64`) |

Le chemin se choisit avec `PRINTR_CLAUDE_BACKEND` :

- `auto` (par défaut) : l'API si `ANTHROPIC_API_KEY` est définie, sinon la CLI si elle est installée ;
- `api` : toujours l'API (sans clé, les blocs Claude sont indisponibles) ;
- `cli` : toujours la CLI, même si une clé est définie.

Avec la CLI, printr lance `claude -p` en le réduisant à un simple appel au modèle : ni outils, ni
réglages, ni `CLAUDE.md`, ni serveurs MCP, ni skills, ni session gardée. Le contexte envoyé reste
ainsi aussi léger qu'avec l'API. La CLI utilise sa propre connexion : printr lui retire
`ANTHROPIC_API_KEY`. Les appels passent un par un, et un appel qui dépasse
`PRINTR_CLAUDE_TIMEOUT_SECS` (120 s par défaut) est abandonné.

Le chemin API parle à l'API Messages : par défaut à `https://api.anthropic.com`, ou à n'importe
quel point de terminaison compatible — un serveur d'inférence local, par exemple — défini dans
`ANTHROPIC_BASE_URL`. La clé, si elle est définie, est envoyée en `x-api-key` — un serveur local
n'en a pas besoin — : ne pointez `ANTHROPIC_BASE_URL` que vers un serveur de confiance.

| Variable | Rôle |
|---|---|
| `ANTHROPIC_API_KEY` | Clé de l'API Claude (inutile pour un serveur compatible dans `ANTHROPIC_BASE_URL`) |
| `ANTHROPIC_BASE_URL` | Point de terminaison de l'API Messages (par défaut `https://api.anthropic.com`) : tout serveur compatible Claude, y compris les serveurs d'inférence locaux (de confiance) |
| `PRINTR_CLAUDE_BACKEND` | `auto`, `api` ou `cli` |
| `PRINTR_CLAUDE_CLI` | Programme de la CLI : nom cherché dans le `PATH` ou chemin complet (`claude` par défaut) |
| `CLAUDE_CODE_OAUTH_TOKEN` | Jeton de la CLI, pour une machine sans session ouverte (le service systemd) |
| `PRINTR_CLAUDE_MODEL` | Modèle, `claude-haiku-5-5` par défaut (les deux chemins) |
| `PRINTR_CLAUDE_TIMEOUT_SECS` | Délai d'un appel Claude, 120 s par défaut (les deux chemins) ; à augmenter pour un modèle local lent |

Sur un poste où tu es déjà connecté à Claude Code, il n'y a rien à faire : sans clé API, printr
trouve `claude` et l'utilise. Pour le service systemd, voir
[Claude par l'abonnement](#claude-par-labonnement) dans l'installation. La bannière du serveur dit
quel chemin est utilisé (`claude-haiku-5-5 (CLI)`), comme `/api/status` (`"claude_backend": "CLI"`).

Les contenus dépendant d'une date sont **mis en cache** : réimprimer le même ticket redonne les
mêmes résultats, sans nouvel appel. `--refresh` force une nouvelle génération. Le cache vit dans
`~/.cache/printr` (ou `$PRINTR_CACHE_DIR`, ou `/var/cache/printr` avec le service systemd).
`PRINTR_DEBUG=1` affiche les réponses brutes de l'API.

## Langue

Printr parle français par défaut. `PRINTR_LANG=en` (dans `/etc/printr.env` pour le service) passe
toute l'installation en anglais : l'interface web, la ligne de commande, les messages et les
tickets. La langue se choisit aussi sur l'écran du premier compte, ou plus tard dans
[les réglages](#réglages). Les blocs à contenu (énigmes, citations, mots mêlés, défis sportifs, bons à offrir…) ont
leur propre liste anglaise, et Claude écrit l'horoscope et le mot du jour en anglais.

Les **mots fléchés** viennent du dictionnaire anglais de CrossWords, avec des définitions anglaises
(son cache `definitions-en.txt`, complété par Claude). L'**horoscope Barnum**, hors ligne, s'écrit
aussi en anglais à partir de Barnum 0.3 (un Barnum plus ancien l'imprime en français). Seule la
**fête du jour** reste en français, faute de source anglaise ; les **jours fériés** sont ceux de
France, nommés en anglais.

## Interface web

`printr serve` sert une interface web pensée pour le téléphone comme pour l'ordinateur :

<p align="center">
  <img src="shots/mobile-home.png" alt="Accueil : les tickets de la famille" width="250">
  &nbsp;
  <img src="shots/mobile-preview.png" alt="Aperçu d'un ticket avant impression" width="250">
  &nbsp;
  <img src="shots/mobile-message.png" alt="Petit mot avec une photo" width="250">
</p>

- **Composer** : ajouter des blocs (à la fin ou entre deux autres), les régler et les réordonner en les faisant glisser par leur poignée, avec l'aperçu du ticket en direct.
  Les blocs Claude (horoscope, mot du jour) n'appellent pas Claude en aperçu : ils ne sont
  rédigés qu'à l'impression.
- **Mes tickets** : les tickets enregistrés (presets), à imprimer en un geste, à partager avec
  la famille et à **planifier** (jours et heure). Une échéance manquée de plus de 15 minutes
  n'est pas rattrapée.
- **Petit mot** : une photo prise avec le téléphone, un dessin (cœur, étoile…), quelques mots.
- **Courses** : la liste partagée de la famille ; chacun ajoute ce qui manque, on l'imprime en
  partant (et elle se vide toute seule). Un raccourci iOS peut aussi y ajouter des articles
  avec `POST /api/shopping` et le jeton.
- **Historique** : qui a imprimé quoi, et les éventuelles erreurs.

Chaque membre de la famille a son compte. Le premier se crée sur l'écran d'accueil de
l'appli (depuis le réseau de la maison) ; les suivants depuis le menu **Mon compte**, où chacun
change aussi son mot de passe. Le premier compte est administrateur : lui seul peut en supprimer.
En ligne de commande, même pendant que le serveur tourne (il relit ses données) :

```sh
printr user add Camille       # demande le mot de passe (ou le lit sur l'entrée standard)
printr user passwd Camille
printr user remove Camille
printr user list
```

**Imprimante absente** (débranchée, éteinte, hors réseau) : l'appli l'affiche, et les tickets, déjà mis
en page, attendent son retour puis sortent tout seuls, planifications comprises (« sorti à 9 h 12, en
retard » dans l'historique). Au-delà de 12 heures (`PRINTR_QUEUE_HOURS`), un ticket en attente est
abandonné : le brief du matin n'a plus d'intérêt le soir.

### Premier lancement

Sur une installation neuve, l'écran d'accueil propose de créer le premier compte, celui de
l'administrateur, et de choisir la langue. Un court assistant suit :

<p align="center">
  <img src="shots/mobile-wizard.png" alt="Assistant : l'imprimante trouvée sur le réseau, et la question des accents" width="250">
  &nbsp;
  <img src="tickets/test.png" alt="Ticket de test avec des lettres accentuées et le signe euro" width="250">
</p>

1. **L'imprimante** : Printr liste les imprimantes USB (`/dev/tm88`, `/dev/usb/lp*`) et celles qui
   répondent sur le port 9100 dans le réseau local ; une autre adresse peut être saisie. Un ticket
   de test la vérifie : si les accents sortent en symboles bizarres, « Non » le réimprime avec
   l'autre page de code (PC850, pour les imprimantes sans PC858), et garde celle qui marche.
2. **La ville**, pour la météo, la qualité de l'air et les horaires du soleil.
3. **Claude** (facultatif) : une clé d'API Anthropic, pour les histoires, l'horoscope, les recettes
   et le mot du jour.

Tout est enregistré à la fin, d'un coup, et chaque étape peut être passée. Il reste à ajouter la
famille depuis **Mon compte**.

### Réglages

L'administrateur modifie l'installation depuis **Mon compte → Réglages** : langue, ville, unités de la météo (°C et km/h, ou °F et mph),
imprimante (avec détection et ticket de test), page de code, clé, modèle et délai de Claude, MQTT, jeton
d'API, tickets par heure, attente des tickets quand l'imprimante est absente, et vérification des
nouvelles versions.

<p align="center"><img src="shots/mobile-settings.png" alt="Réglages" width="250"></p>

`ANTHROPIC_BASE_URL` n'en fait pas partie, exprès : la clé Claude part vers cette adresse, et
l'appli ne livre jamais la clé. Les réglages sont enregistrés avec les données (`printr.json`) et appliqués par un redémarrage
rapide, automatique sous systemd ou Docker (sinon au prochain démarrage). Ce que fixent
`/etc/printr.env` ou l'environnement du conteneur l'emporte : l'appli le montre en lecture seule,
avec la mention « Fixé dans /etc/printr.env ». Les secrets (clé Claude, adresse MQTT, jeton) ne
repartent jamais vers l'appli, qui indique seulement s'ils sont définis.

### Sauvegarde

Depuis **Mon compte**, l'administrateur télécharge une **sauvegarde** : comptes, tickets et leurs
planifications, historique, liste de courses, anniversaires de la famille, réglages et photos, dans un
seul fichier JSON. Elle contient la clé Claude et les mots de passe (hachés) : à garder en lieu sûr.
**Restaurer** remplace tout cela par le contenu du fichier ; l'état actuel est d'abord gardé dans
`backups/`, à côté des données (les cinq derniers), et chacun se reconnecte.

Sur une installation neuve, l'écran du premier compte propose aussi de restaurer une sauvegarde
(depuis le réseau de la maison seulement, comme le premier compte) : une nouvelle carte microSD, puis
le Printr de la famille tel qu'il était.

### Données

Les données (comptes, presets, historique, photos, réglages) vivent dans `$PRINTR_DATA_DIR`, sinon
`~/.local/share/printr`, ou `/var/lib/printr` avec le service systemd. Pour travailler sur
l'interface sans recompiler : `PRINTR_WEB_DIR=web printr serve`.

## Serveur HTTP

```sh
printr serve --listen 0.0.0.0:8080 --token $(openssl rand -hex 24)   # jeton : optionnel
```

| Route | Corps | Effet |
| --- | --- | --- |
| `GET /` | — | Vérifie que le serveur tourne |
| `POST /print` | Un ticket JSON | Imprime le ticket |
| `POST /todo` | `{"title"?, "items"?: [...], "text"?: "une\nligne\npar\nélément"}` | Imprime la date et une liste à cocher |
| `POST /api/shopping` | `{"text": "lait\npain"}` | Ajoute des articles à la liste de courses (un par ligne) |
| `GET /api/status` | — | État de l'imprimante (`printer_status`), file d'attente, jauge de papier (`paper`), version |
| `POST /api/paper/roll` | `{"length_m"?: 70}` | Un rouleau neuf est posé : remet la jauge à zéro (longueur en mètres, celle du rouleau précédent si absente) |

Pour les scripts, les `POST` exigent l'en-tête `Authorization: Bearer <jeton>` (défini avec `--token`). Ajouter `?preview` renvoie
l'aperçu texte sans imprimer. La réponse est `{"ok": true, "errors": [...]}`, où `errors` liste
les blocs en échec. Les requêtes sont traitées une par une : deux impressions ne se mélangent jamais.

```sh
curl -X POST http://raspberrypi:8080/print -H "Authorization: Bearer $PRINTR_TOKEN" \
     --data @examples/matin.json
```

### Imprimer ses Rappels iOS

Apple ne propose pas d'API pour Rappels, mais l'app Raccourcis peut envoyer la liste au Pi.
Dans un nouveau raccourci :

1. **Rechercher des rappels** : filtre « Liste est *Courses* » et « N'est pas terminé ».
2. **Combiner le texte** : entrée = les rappels, séparateur = « Nouvelle ligne ».
3. **Obtenir le contenu de l'URL** :
   - URL : `http://<ip-du-pi>:8080/todo`, méthode **POST** ;
   - en-tête `Authorization` = `Bearer <jeton>` ;
   - corps **JSON** : `title` (texte) = `Courses`, `text` (texte) = *Texte combiné*.
4. Optionnel : **Afficher la notification** avec le résultat.

Pour l'automatiser : Raccourcis > Automatisation > Heure de la journée, puis « Exécuter
immédiatement ». Le téléphone doit pouvoir joindre le Pi (même Wi-Fi, ou un VPN type Tailscale).

### Node-RED

Le paquet [`node-red-contrib-printr`](https://www.npmjs.com/package/node-red-contrib-printr)
([sources](https://github.com/XNinety9/node-red-contrib-printr)) ajoute des nœuds Printr à Node-RED. On
ne donne que l'adresse du Pi et le jeton, jamais une route :

| Nœud | Rôle |
| --- | --- |
| **printr print** | imprime un ticket JSON, ou un texte |
| **printr to-do** | imprime une liste à cocher avec la date |
| **printr shopping** | ajoute des articles à la liste de courses |
| **printr ticket** | imprime un ticket enregistré, par son nom (tickets partagés seulement) |
| **printr status** | lit l'état de l'imprimante : prête ou non, tickets en attente |

1. L'installer depuis la palette de Node-RED (**Gérer la palette → Installer**), ou avec
   `npm install node-red-contrib-printr` dans le dossier utilisateur de Node-RED, puis redémarrer
   Node-RED.
2. Définir un jeton sur le Pi : `PRINTR_TOKEN=…` dans `/etc/printr.env` (16 caractères ou plus, par
   exemple `openssl rand -hex 24`), puis `sudo systemctl restart printr`.
3. Dans un nœud **printr**, créer la configuration du serveur : URL `http://<ip-du-pi>:8080`, et le jeton.

Chaque nœud ajoute `msg.printr` (`printed`, `queued`, `stalled`, `ok` ou `error`) : un ticket en attente
attend l'imprimante, ce n'est pas une erreur. Le paquet fournit un flow d'exemple (**Menu → Importer →
Exemples**).

## MQTT

Les domotiques (Node-RED, Jeedom, Home Assistant…) peuvent imprimer en publiant sur un broker MQTT, et y
lire l'état de l'imprimante. Renseigner le broker dans `/etc/printr.env`, puis redémarrer le service :

```sh
PRINTR_MQTT_URL=mqtt://utilisateur:motdepasse@192.168.1.10:1883   # mqtts:// pour TLS
PRINTR_MQTT_PREFIX=printr                                         # facultatif, printr par défaut
PRINTR_MQTT_DISCOVERY=homeassistant                               # facultatif : préfixe de découverte Home Assistant, ou off
```

| Sujet | Sens | Message |
| --- | --- | --- |
| `printr/print` | vers Printr | un ticket JSON, ou un texte |
| `printr/print/ticket` | vers Printr | le nom d'un ticket enregistré (tickets partagés seulement) |
| `printr/todo` | vers Printr | `{"title"?, "items"}`, ou une chose par ligne |
| `printr/shopping` | vers Printr | des articles pour la liste de courses, un par ligne |
| `printr/paper/roll` | vers Printr | un rouleau neuf est posé (facultatif : sa longueur en mètres) |
| `printr/result` | de Printr | l'issue de chaque message : `printed`, `queued`, `stalled`, `added` ou `error` |
| `printr/status` | de Printr, retenu | `printer_ready`, `printer_status` (`ready`, `cover_open`, `paper_out`, `missing`), `stalled`, `queued`, `paper` (jauge : `remaining_mm`, `percent`, `low`…), `last_ticket`, `version` |
| `printr/availability` | de Printr, retenu | `online`, ou `offline` quand Printr disparaît |

```sh
mosquitto_pub -h 192.168.1.10 -t printr/print -m "On a sonné à la porte"
mosquitto_pub -h 192.168.1.10 -t printr/print/ticket -m "Brief du matin"
```

Les règles de l'API HTTP s'appliquent : 60 blocs au plus, le quota de l'heure (les impressions
apparaissent « par MQTT » dans l'historique), et la file d'attente quand l'imprimante est absente. MQTT
n'a pas de jeton : **tout client autorisé sur le broker peut imprimer**, c'est donc sur le broker qu'on
restreint l'accès (comptes, ACL). Les messages envoyés pendant que Printr est déconnecté ne sont pas
gardés : un brief du matin ne sort pas le soir.

### Certificats TLS

`mqtt://` (sans TLS) fonctionne avec n'importe quel broker de ton réseau. `mqtts://` vérifie le
certificat du broker : qu'il est signé par une autorité de confiance, et que son nom correspond à
l'adresse. Printr fait confiance par défaut aux autorités publiques (Let's Encrypt, par exemple). Pour
ton propre certificat, donne-le à Printr avec `PRINTR_MQTT_CA` : la vérification reste complète, Printr
accepte seulement ce certificat en plus des publics.

- **Sans `PRINTR_MQTT_CA`**, un certificat auto-signé ou privé est refusé : le journal affiche
  `MQTT … injoignable` avec `UnknownIssuer`, et Printr continue de réessayer en arrière-plan.
- **Certificat auto-signé du broker** : crée-le avec `CA:FALSE`, sinon il est refusé même avec
  `PRINTR_MQTT_CA` (`CaUsedAsEndEntity`). Le nom doit correspondre à l'adresse de `PRINTR_MQTT_URL` :

  ```sh
  openssl req -x509 -newkey rsa:2048 -nodes -keyout mqtt-key.pem -out mqtt-cert.pem -days 3650 \
    -subj "/CN=192.168.1.200" -addext "subjectAltName=IP:192.168.1.200" -addext "basicConstraints=critical,CA:FALSE"
  ```

  Donne `mqtt-cert.pem` et `mqtt-key.pem` au broker, et `mqtt-cert.pem` à Printr comme `PRINTR_MQTT_CA`.
  Sur un broker Mosquitto, l'écouteur TLS s'écrit :

  ```text
  listener 8883
  certfile /etc/mosquitto/certs/mqtt-cert.pem
  keyfile /etc/mosquitto/certs/mqtt-key.pem
  ```

  Puis utilise `PRINTR_MQTT_URL=mqtts://192.168.1.200:8883` (l'adresse doit être celle du certificat).
- **Autorité privée** : signe le certificat du broker avec ton autorité, et donne à Printr le certificat
  de l'autorité comme `PRINTR_MQTT_CA`.
- **Ne désactive jamais la vérification** : le chiffrement ne protégerait alors plus rien.

Printr lit le fichier au démarrage : redémarre le service après un changement. Le fichier doit être
lisible par l'utilisateur `printr`.

## Home Assistant

Deux façons de faire entrer Printr dans Home Assistant. **Choisissez-en une** : chacune crée un appareil
Printr.

| | Intégration Printr | Découverte MQTT |
| --- | --- | --- |
| Il faut | l'[intégration de Matteo Penaud](https://github.com/Matteo-Penaud/printr-ha-integration) et le jeton des scripts | un broker MQTT et l'intégration MQTT de Home Assistant |
| Pour imprimer | les actions `printr.print_ticket`, `print_text`, `print_todo`, `print_states` ; `notify.send_message` | un bouton par ticket partagé, un champ « Imprimer un texte », `mqtt.publish` |
| Dans Home Assistant | dernière impression, tickets du jour, échec d'impression, mise à jour disponible, la liste de courses en liste de tâches | état de l'imprimante, papier restant, papier presque fini, tickets en attente, dernier ticket, bouton Nouveau rouleau |

L'intégration passe par l'API HTTP : définissez un jeton (`PRINTR_TOKEN` dans `/etc/printr.env`), puis
ajoutez l'intégration dans *Paramètres → Appareils et services* avec l'adresse du serveur et ce jeton.
Si Printr est aussi relié à MQTT pour d'autres usages, mettez `PRINTR_MQTT_DISCOVERY=off`, sinon Home
Assistant voit un deuxième appareil Printr.

### Avec MQTT (découverte)

Avec l'[intégration MQTT](https://www.home-assistant.io/integrations/mqtt/) branchée sur le même broker,
Printr apparaît tout seul dans Home Assistant comme un appareil **Printr** (découverte MQTT), sans rien
configurer :

- des **capteurs** : état de l'imprimante (`ready`, `cover_open`, `paper_out`, `missing`), problème,
  papier restant (% et mètres), papier presque fini, tickets en attente, dernier ticket ;
- un bouton **Nouveau rouleau**, et un champ **Imprimer un texte** ;
- **un bouton par ticket partagé**, ajouté, renommé ou retiré au fil des tickets.

La découverte utilise le préfixe `homeassistant` ; `PRINTR_MQTT_DISCOVERY` le change,
ou la coupe avec `off` : Printr retire alors l'appareil qu'il avait annoncé, qui ne reste pas à côté d'une
autre intégration. Deux Printr sur un même broker doivent avoir des `PRINTR_MQTT_PREFIX`
différents : chacun est son propre appareil.

**Un ticket de l'appli dans une automatisation** : dans le composeur, « Afficher la configuration du
ticket » (administrateur) donne le ticket en YAML, prêt à coller dans une automatisation ou un script
(mode YAML), ou en JSON pour l'API HTTP, MQTT et `printr print`. En YAML, deux formes : l'action
`printr.print_ticket` de l'intégration, ou l'action `mqtt.publish` qui publie le ticket sur
`printr/print`, proposée quand Printr est relié à MQTT.

### Exemples d'automatisations

Les noms d'entités ci-dessous sont des exemples : les vrais sont dans *Paramètres → Entités*.

**On a sonné** (intégration) :

```yaml
triggers:
  - trigger: state
    entity_id: binary_sensor.sonnette
    to: "on"
actions:
  - action: notify.send_message
    target:
      entity_id: notify.printr_ticket
    data:
      title: Sonnette
      message: "On a sonné à {{ now().strftime('%H:%M') }}"
```

**Le lave-linge a fini** (MQTT, sans intégration) :

```yaml
triggers:
  - trigger: numeric_state
    entity_id: sensor.puissance_lave_linge
    below: 3
    for: "00:05:00"
actions:
  - action: mqtt.publish
    data:
      topic: printr/print
      payload: "Le lave-linge a fini !"
```

**La maison ce matin** (intégration) : la température dehors, la consommation d'hier et l'alarme, sur un
seul ticket.

```yaml
actions:
  - action: printr.print_states
    data:
      title: Ce matin
      entity_id:
        - sensor.temperature_exterieure
        - sensor.consommation_hier
        - alarm_control_panel.maison
```

**Papier presque fini, prévenir le téléphone** (découverte MQTT) :

```yaml
triggers:
  - trigger: state
    entity_id: binary_sensor.printr_papier_presque_fini
    to: "on"
actions:
  - action: notify.mobile_app_mon_telephone
    data:
      message: "Printr n'a presque plus de papier."
```

Autres idées : un colis livré, un rappel de médicament, ou un mot personnalisé pour la première personne
qui passe le matin devant la tablette du couloir (reconnaissance faciale, puis `print_ticket` avec son
prénom).

## Sécurité

L'appli peut être ouverte sur Internet derrière un reverse proxy HTTPS (Caddy, nginx…). Elle
se protège ainsi :

- **Comptes** : mots de passe hachés avec Argon2 (8 caractères minimum), cookie de session
  `HttpOnly`, `SameSite=Lax` et `Secure` en HTTPS. Le premier compte, administrateur, ne se crée
  que depuis le réseau local ; seul l'administrateur peut supprimer un compte.
- **Essais de mot de passe** : cinq échecs par adresse en quinze minutes, puis blocage ; une
  seule vérification à la fois, pour que des essais en masse n'épuisent pas la mémoire.
  Derrière le proxy, l'adresse du client est lue dans `X-Forwarded-For` (dernière entrée).
- **Requêtes d'un autre site** refusées (en-tête `Origin`), même depuis un sous-domaine voisin.
- **Réseau local hors d'atteinte** : depuis l'appli ou un script, les blocs (images, flux RSS,
  agendas…) ne joignent que des adresses publiques, redirections comprises, et ne lisent
  aucun fichier de la machine. En ligne de commande, tout reste permis.
- **Abus** : 60 blocs au plus par ticket, images décodées avec des limites de taille, et
  `PRINTR_MAX_TICKETS_PER_HOUR` impressions par personne et par heure (20 par défaut).
- **Service** sans privilèges, mémoire plafonnée et système en lecture seule (voir
  [`deploy/printr.service`](../deploy/printr.service)) ; politique de sécurité du contenu (CSP)
  sur l'interface.

**Mises à jour** : une fois par jour (`PRINTR_UPDATE_INTERVAL` minutes pour changer, 5 au
minimum), printr regarde sur GitHub si une version plus récente est publiée et le signale à
l'administrateur (« Mon compte »). Par défaut, rien n'est téléchargé ni installé tout seul : la mise
à jour reste un geste humain (`scripts/deploy.sh`), après lecture des nouveautés.
`PRINTR_UPDATE_CHECK=off` coupe la vérification.

Les **mises à jour automatiques** se choisissent : l'assistant les propose, et elles s'activent
dans les réglages (`PRINTR_AUTO_UPDATE=auto`). Printr n'installe alors une nouvelle version **que si
elle est signée** avec la clé du mainteneur, qui ne quitte jamais son ordinateur : la signature
porte sur la liste des empreintes (`SHA256SUMS`), et le programme téléchargé doit avoir la bonne
empreinte, se lancer et annoncer la version attendue. Un compte GitHub compromis ne peut donc rien
pousser sur les machines. Le remplacement se fait la nuit (`PRINTR_UPDATE_WINDOW`, `03:00-05:00`
par défaut), une fois l'imprimante libre ; l'ancien programme reste dans `printr.old`. Si la
nouvelle version ne répond pas dans les deux minutes qui suivent son démarrage, la garde lancée par
le service avant chaque démarrage ([`deploy/printr-update-guard.sh`](../deploy/printr-update-guard.sh))
remet l'ancienne, et cette version n'est plus jamais essayée. Chaque mise à jour apparaît dans
l'historique. Dans un conteneur, rien ne s'installe : c'est l'image qu'on met à jour. Le service
peut écrire dans `/opt/printr` pour cela, et seulement là.

Côté machine, il reste conseillé de n'accepter que les clés en SSH, et de vérifier que le
pare-feu IPv6 de la box ne laisse pas entrer les connexions vers le réseau local.

## Tester sans imprimante (émulateur)

[emupos](https://pypi.org/project/emupos/) simule l'imprimante et rend chaque ticket en PNG + texte.
Le dossier `emulator/` contient un profil TM-T88V (512 points, 180 dpi).

Pour générer le ticket complet (tous les blocs) en une commande :

```sh
scripts/demo.sh                          # examples/complet.json par défaut
scripts/demo.sh examples/extras.json     # ou n'importe quel ticket
# → affiche le chemin du rendu, emulator/receipts/<id>.png (et .txt)
```

Le script lance l'émulateur, imprime le ticket, attend le rendu puis arrête l'émulateur.
Les blocs Claude demandent `ANTHROPIC_API_KEY` dans l'environnement, ou la CLI Claude.

À la main, dans deux terminaux :

```sh
cd emulator && uvx emupos run                # terminal 1
cargo run -- --tcp 127.0.0.1:9100 test       # terminal 2
# → emulator/receipts/*.png et *.txt
```

## Développer sous Windows (dev container)

Le dossier `.devcontainer/` fournit un environnement Linux avec Rust et uv : dans VS Code
(extension Dev Containers, Docker Desktop lancé), **Reopen in Container**. Tout se fait ensuite
dans le terminal du conteneur, comme sur le Pi :

```sh
cargo run -- --preview print ticket.json           # aperçu dans le terminal
scripts/demo.sh ticket.json                         # rendu PNG via l'émulateur
cargo run -- --tcp host.docker.internal:9101 print ticket.json   # vraie imprimante
```

### Relais PowerShell vers l'imprimante USB

L'imprimante USB reste branchée sur Windows. Pour l'atteindre depuis le conteneur, le script
[`scripts/windows-relay.ps1`](../scripts/windows-relay.ps1) écoute sur `127.0.0.1:9101` côté Windows
et transmet chaque connexion reçue à la file d'impression, en brut (RAW, sans le rendu du pilote).
Un ticket = une connexion : il est imprimé à la fermeture de celle-ci.

Dans un terminal PowerShell **Windows**, à la racine du dépôt, et le laisser ouvert :

```powershell
powershell -ExecutionPolicy Bypass -File scripts\windows-relay.ps1
# Relais 127.0.0.1:9101 -> 'EPSON TM-T88V Receipt' (Ctrl+C pour arrêter)
```

Options : `-Printer "nom"` si la file Windows porte un autre nom (liste : `Get-Printer | Select Name`),
`-Port 9102` pour un autre port. Chaque ticket reçu s'affiche avec sa taille en octets.

Puis, dans le conteneur :

```sh
cargo run -- --tcp host.docker.internal:9101 print examples/matin.json
```

En cas de souci :

- **`impossible de se connecter à host.docker.internal:9101`** : le relais ne tourne pas (ou sur un
  autre port). Vérifier côté Windows avec `Test-NetConnection 127.0.0.1 -Port 9101`.
- **Le script s'arrête au démarrage** : la file `-Printer` n'existe pas ; prendre le nom exact
  donné par `Get-Printer`.
- **Connexion acceptée mais rien ne sort** : vérifier que l'imprimante est allumée et que la file
  Windows n'est pas en pause ou en erreur.

## Site et illustrations

Le site vit dans [`docs/`](./) : la page d'accueil, et des pages de documentation générées
depuis ce guide et le [guide de contribution](../CONTRIBUTING.fr.md), pour ne jamais s'en écarter.
GitHub Actions le construit et le publie sur GitHub Pages à chaque modification sur `master`.
Pour le voir en local :

```sh
uvx --with markdown --with pymdown-extensions python docs/build.py && python3 -m http.server -d _site
```

Ses illustrations sont générées avec des données fictives uniquement :

```sh
scripts/illustrations.sh                                # tout, dans les deux langues, puis le site
scripts/illustrations.sh --sans-claude                  # pareil, sans aucun appel à Claude
docs/demo/photo-session.sh                              # captures de l'appli + tickets de l'émulateur
docs/demo/setup-session.sh                              # mise en route : point d'accès, assistant, réglages
uv run --with pillow python docs/brand/make-assets.py   # déclinaisons du logo et icônes de l'appli
```

## Imprimantes supportées

Printr parle **ESC/POS** et met les tickets en page pour du **papier de 80 mm** : 512 points de large à
180 dpi, 42 colonnes en police A et 56 en police B, images envoyées en raster (`GS v 0`), page de code
PC858 (accents et €) et massicot (`GS V`). Si les accents manquent à l'impression (certains clones n'ont pas la page 19), `PRINTR_CODEPAGE=850` sélectionne PC850 (`ESC t 2`) : le `€` s'imprime alors « EUR ».

| Imprimante | Prise en charge |
|---|---|
| **Epson TM-T88V** (USB) | ✅ Testée tous les jours : l'imprimante de référence |
| Epson TM-T88IV, TM-T88VI, TM-T88VII | 🟡 Devraient marcher telles quelles (mêmes commandes, mêmes 180 dpi), pas encore testées |
| MUNBYN 047P-USEW (Ethernet) | 🟡 Imprime avec `PRINTR_CODEPAGE=850` (pas de page PC858 : les accents manquent sans) ; testée en v1.11.0 |
| Autres imprimantes ESC/POS 80 mm à 203 dpi (Epson TM-T20, TM-m30, la plupart des autres marques) | 🟡 Devraient imprimer, un peu plus étroit et plus petit que sur une TM-T88 ; les images demandent la prise en charge du raster |
| Imprimantes 58 mm | ❌ Non prises en charge : les tickets font 512 points de large |
| Imprimantes sans ESC/POS (mode ligne Star, étiqueteuses, CUPS seulement) | ❌ Non prises en charge |

Trois façons de joindre l'imprimante :

- **USB** : `--device` (`/dev/usb/lp0` par défaut, par le pilote Linux `usblp`). Printr y lit aussi
  l'état de l'imprimante : capot ouvert, plus de papier, débranchée.
- **Réseau** : `--tcp hôte:9100` (port brut des imprimantes réseau). Printr sait seulement si
  l'imprimante répond.
- **Fichier** : `--dump` écrit les octets ESC/POS, pour tester sans imprimante (voir
  l'[émulateur](#tester-sans-imprimante-émulateur)).

Vous avez essayé Printr sur une autre imprimante ? Dites-le dans les
[discussions](https://github.com/XNinety9/Printr/discussions), avec ce qui a marché ou non : ce tableau
s'enrichira de vos retours.

### Papier

Printr utilise des rouleaux ordinaires de **papier thermique de 80 mm** ; un rouleau de 70 mm de
diamètre fait environ 70 m. La plupart des papiers thermiques contiennent un révélateur chimique qui
réagit à la chaleur : souvent du **BPA** (bisphénol A), interdit dans le papier thermique dans l'Union
européenne depuis janvier 2020, et de plus en plus du **BPS**, son remplaçant, qui n'est pas interdit
et pose les mêmes questions. Comme ce papier passe entre toutes les mains, y compris celles des
enfants, préférez les rouleaux **« sans phénol »**, ou sans BPA ni BPS : ils coûtent un peu plus cher,
ce qui ne compte guère à l'échelle d'une famille. En général, le papier thermique ne va pas au recyclage du
papier.

## Installation

Deux façons :

- sur un **Raspberry Pi**, l'[image prête à flasher](#image-prête-à-flasher) : rien à installer,
  même le Wi-Fi se règle depuis le téléphone ;
- sur **n'importe quel Linux**, Raspberry Pi compris, le [script d'installation](#script-dinstallation),
  qui ajoute Printr comme service systemd.

### Image prête à flasher

Chaque [version](https://github.com/XNinety9/Printr/releases) est accompagnée d'une image pour
Raspberry Pi 3, 4, 5 et Zero 2 W (64 bits) : Raspberry Pi OS Lite avec Printr prêt à l'emploi
(`printr-X.Y.Z-raspberrypi-arm64.img.xz`).

1. L'écrire sur une carte microSD avec [Raspberry Pi Imager](https://www.raspberrypi.com/software/) :
   **Choisir l'OS → Use custom**. Ou faire apparaître Printr dans la liste d'Imager : dans ses
   options, indiquer comme dépôt de contenu
   `https://github.com/XNinety9/Printr/releases/latest/download/printr-imager.json`. Sa personnalisation (roue dentée) est facultative : un compte et
   SSH, pour se connecter à la machine ; le Wi-Fi, pour se passer de la
   [première connexion au réseau](#première-connexion-au-réseau).
2. Brancher l'imprimante en USB, et un câble Ethernet s'il y en a un, puis allumer.
3. En Ethernet, ou avec le Wi-Fi donné à Imager, l'appli est sur `http://printr.local` (ou à
   l'adresse indiquée par la box). Sinon, Printr imprime un ticket pour régler le Wi-Fi.

L'image sert l'appli sur le port 80, sous le nom `printr`, avec la langue et le clavier français,
le fuseau de Paris et le pays Wi-Fi FR (Imager les change), Barnum pour l'horoscope hors ligne, et
ni compte ni SSH sauf si Imager les ajoute. Tout le reste se règle dans l'appli : le
[premier lancement](#premier-lancement), puis les [réglages](#réglages). Pour construire l'image
soi-même : [image/README.md](../image/README.md).

### Première connexion au réseau

Sans réseau une minute après l'allumage, Printr ouvre son propre Wi-Fi, **Printr-XXXX**, et imprime
un ticket :

<p align="center">
  <img src="tickets/wifi.png" alt="Ticket avec deux QR codes : rejoindre le Wi-Fi de Printr, puis ouvrir la page de configuration" width="250">
  &nbsp;
  <img src="shots/mobile-wifi.png" alt="Page de configuration : les réseaux Wi-Fi à portée" width="250">
</p>

1. Scanner le premier QR code pour rejoindre ce Wi-Fi (son mot de passe est imprimé dessous).
2. La page de configuration s'ouvre en général toute seule, comme sur un Wi-Fi d'hôtel ; sinon,
   scanner le second QR code (`http://10.42.0.1`).
3. Choisir le Wi-Fi de la maison et taper son mot de passe : Printr le rejoint et imprime un second
   ticket avec l'adresse de l'appli. Remettre le téléphone sur le Wi-Fi de la maison, et ouvrir
   cette adresse.

Un mauvais mot de passe ? Printr le dit sur un nouveau ticket et rouvre son Wi-Fi, avec le même nom
et le même mot de passe. Ce Wi-Fi est protégé (WPA2) par le mot de passe du ticket, qui reste le
même d'un démarrage à l'autre, et n'existe que tant que Printr n'a pas de réseau.

Quand le Wi-Fi de la maison est déjà connu mais absent au démarrage (après une coupure de courant,
la box redémarre souvent moins vite que le Pi), Printr attend cinq minutes avant d'ouvrir son
propre Wi-Fi, puis retente celui de la maison toutes les cinq minutes. Certaines box bloquent les
noms `.local` entre appareils en Wi-Fi : c'est pourquoi le ticket donne d'abord l'adresse IP.

Sous le capot : `PRINTR_WIFI_SETUP=1` (posé par l'image) active ce mode ; le point d'accès tourne
sur hostapd en WPA2 classique (celui de NetworkManager est refusé par les téléphones sur la puce
Wi-Fi des Raspberry Pi), NetworkManager s'occupe du reste.

### En une commande

Sur une machine déjà en service (Raspberry Pi OS, ou tout Linux avec systemd), depuis son compte :

```sh
curl -fsSL https://xninety9.github.io/Printr/install.sh | bash
```

La commande télécharge l'archive de la dernière version pour cette machine, vérifie la signature de
la release (les mêmes clés que la mise à jour automatique, avec `openssl`) et l'empreinte de
l'archive, puis lance son `install.sh` ; le mot de passe `sudo` est demandé une fois. La suite se
passe dans le navigateur, avec l'assistant du [premier lancement](#premier-lancement). La relancer
met Printr à jour. `curl … | bash -s -- --download-only` télécharge et vérifie seulement.

### Script d'installation

printr s'installe comme service systemd sur n'importe quel Linux : Raspberry Pi (Raspberry Pi
OS Lite 64 bits conseillé), serveur ou PC, en ARM 64 bits, ARM 32 bits ou x86_64. L'imprimante
peut être en USB ou sur le réseau.

```sh
scripts/deploy.sh pi@printer.local      # machine distante, par SSH (clé) ; --port N si besoin
scripts/deploy.sh --local               # cette machine
```

Le script détecte l'architecture de la machine, compile pour elle (avec
[`cross`](https://github.com/cross-rs/cross), pour une glibc compatible), envoie le programme
et lance [`deploy/install.sh`](../deploy/install.sh), qui demande le mot de passe `sudo` **une
seule fois** et la clé Claude (facultative, voir aussi
[Claude par l'abonnement](#claude-par-labonnement)). L'installation :

- crée l'utilisateur système `printr` et ajoute ton compte aux groupes `printr` et `lp` ;
- installe le programme dans `/opt/printr` (et la commande `printr`) ;
- installe la règle udev de l'imprimante USB (lien stable `/dev/tm88`) et charge `usblp` ;
- clone [Barnum](https://github.com/XNinety9/Barnum) dans `/opt/barnum` (si `python3` et `git`
  sont là) ;
- crée `/etc/printr.env` et le service, puis vérifie qu'il répond.

**La relancer ne perd rien.** Avant toute modification, elle vérifie que les données sont
lisibles (sinon elle s'arrête sans rien toucher) et les sauvegarde dans
`/var/backups/printr` (les cinq dernières). La configuration existante est gardée : seules les
options nouvelles y sont ajoutées. L'ancien programme reste dans `/opt/printr/printr.old` et les
anciens fichiers système modifiés rejoignent les sauvegardes. Chaque étape dit ce qui a changé.

Ensuite, **plus besoin de sudo** (après s'être déconnecté et reconnecté, pour le groupe) :

```sh
scripts/deploy.sh pi@printer.local      # depuis le PC : mise à jour du programme et redémarrage
nano /etc/printr.env                    # configuration (clé Claude, imprimante, port…)
systemctl restart printr                # redémarrage
journalctl -u printr -f                 # suivre les impressions
```

La plupart de la configuration se change aussi dans l'appli ([réglages](#réglages)) ; ce que fixe
`/etc/printr.env` l'emporte.

`--install` relance l'installation complète (nouveau fichier de service, nouvelle règle udev…).
Pour une imprimante réseau, définir `PRINTR_TCP=adresse:9100` dans `/etc/printr.env`. Sur la
machine, la commande `printr` lit aussi ce fichier : un ticket lancé à la main utilise la même
imprimante et le même accès à Claude que le serveur.

### Claude par l'abonnement

Sans clé API, le service peut passer par la CLI Claude Code et ton abonnement Claude (voir
[Claude : clé API ou abonnement](#claude--clé-api-ou-abonnement)). Le service tourne sous
l'utilisateur `printr`, qui n'a pas accès aux dossiers personnels : la CLI doit être installée
ailleurs, et se connecter par un jeton. Sur la machine (64 bits) :

```sh
curl -fsSL https://claude.ai/install.sh | bash                        # installe la CLI dans ~/.local
sudo install -m 755 "$(readlink -f ~/.local/bin/claude)" /usr/local/bin/claude
claude setup-token                                                    # connexion, affiche un jeton
```

`claude setup-token` peut aussi se lancer sur un autre ordinateur : seul le jeton compte. Puis,
dans `/etc/printr.env` (laisser `ANTHROPIC_API_KEY` vide) :

```sh
PRINTR_CLAUDE_CLI=/usr/local/bin/claude
CLAUDE_CODE_OAUTH_TOKEN=sk-ant-oat01-…
```

et `systemctl restart printr` : la bannière (`journalctl -u printr`) affiche
`claude-haiku-5-5 (CLI)`. La CLI garde ses fichiers dans le dossier de `printr`
(`/var/lib/printr/.claude`). Le service peut utiliser jusqu'à 768 Mo de mémoire, pour laisser de
la place à la CLI. Pour la mettre à jour, recopier le nouveau programme dans `/usr/local/bin`.

**Binaires prêts à l'emploi** : chaque [release](https://github.com/XNinety9/Printr/releases) propose une
archive par machine (`aarch64` pour un Pi 3, 4, 5 ou Zero 2 W en 64 bits, `armv7`, `arm` pour un Pi
Zero W ou Pi 1, `x86_64` pour un PC), avec les fichiers d'installation et un fichier `SHA256SUMS`.
Pas besoin de Rust :

```sh
tar xzf printr-v1.6.0-aarch64-unknown-linux-gnu.tar.gz
cd printr-v1.6.0-aarch64-unknown-linux-gnu && sudo bash install.sh
```

Pour tout faire à la main, compiler pour la machine visée, avec
[`cross`](https://github.com/cross-rs/cross) (Docker) depuis un autre ordinateur, puis lancer
l'installation sur la machine (`--help` pour les options) :

```sh
cross build --release --target aarch64-unknown-linux-gnu     # Pi 3, Pi 4, Pi 5, Zero 2 W (64 bits)
cross build --release --target armv7-unknown-linux-gnueabihf # Pi 2, Pi 3 en 32 bits
cross build --release --target arm-unknown-linux-gnueabihf   # Pi Zero W, Pi 1 (ARMv6)
cargo build --release                                        # cette machine
sudo deploy/install.sh --binary chemin/vers/printr
```


### En conteneur (Docker)

Si la machine fait déjà tourner ses services en conteneurs (Home Assistant, WireGuard…), Printr peut
les rejoindre. [`deploy/Dockerfile`](../deploy/Dockerfile) et [`deploy/compose.yaml`](../deploy/compose.yaml)
emballent le binaire `printr` posé à côté d'eux, avec Barnum, sous un utilisateur sans privilège et
les mêmes garde-fous que le service systemd. Dans une archive de release, le binaire est déjà là ;
depuis le dépôt, copier dans `deploy/` celui compilé pour la machine.

Sur l'hôte, une seule fois : la règle udev de l'imprimante USB et le module `usblp`, comme le ferait
`install.sh` :

```sh
sudo cp 70-tm-t88v.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger
echo usblp | sudo tee /etc/modules-load.d/usblp.conf && sudo modprobe usblp
```

Puis, dans le dossier de l'archive (ou `deploy/`) :

```sh
cp printr.env.example printr.env           # configuration : clé Claude, MQTT, langue…
docker compose up -d --build
docker compose exec printr printr user add Prénom
docker compose logs -f                     # suivre les impressions
```

- L'interface répond sur le port 8080 ; si un autre service l'occupe (« address already in use »),
  mettre `PRINTR_PORT=8090` dans un fichier `.env` à côté de `compose.yaml`.
- Le conteneur voit le `/dev` de l'hôte sous `/host/dev` et imprime sur `/host/dev/tm88`, le lien de
  la règle udev ; en dehors des périphériques de base (`/dev/null`…), il ne peut ouvrir que
  l'imprimante. Éteinte au démarrage, rallumée ou rebranchée, elle est retrouvée comme hors
  conteneur, et les tickets attendent son retour. Si le groupe `lp` de l'hôte n'est pas le 7
  (`getent group lp`), le donner dans `PRINTR_LP_GID`, dans le même fichier `.env`.
- Comptes, historique et photos vivent dans le volume `printr_data`, le cache dans `printr_cache` :
  ils survivent aux mises à jour. Pour mettre à jour, remplacer le binaire puis
  `docker compose up -d --build`.
- Fuseau horaire des planifications : `Europe/Paris` par défaut, `TZ=…` pour un autre.
- MQTT : avec Mosquitto sur la même machine, `PRINTR_MQTT_URL` pointe vers l'adresse de l'hôte (pas
  `localhost`, qui désigne le conteneur). Un certificat `PRINTR_MQTT_CA` se monte en plus dans le
  conteneur.
- La CLI Claude Code n'est pas dans l'image : en conteneur, les blocs Claude passent par
  `ANTHROPIC_API_KEY`.

## Licence

Printr est distribué sous [licence Apache 2.0](../LICENSE) : vous pouvez le réutiliser, le modifier et le
redistribuer, à condition de conserver le fichier [NOTICE](../NOTICE), qui cite l'auteur original et la
bibliothèque CrossWords incluse.
