# Printr

Impression sur une Epson TM-T88V (ESC/POS, USB) depuis un Raspberry Pi, en Rust avec la crate [`escpos`](https://docs.rs/escpos).

## Utilisation

```sh
printr print examples/matin.json    # ticket composé de blocs (voir plus bas)
cat ticket.json | printr print      # idem, depuis l'entrée standard
printr --preview print ticket.json  # aperçu dans le terminal, sans imprimer
printr --refresh print ticket.json  # régénère actualités, soleil, horoscopes et mot du jour
printr -q print ticket.json         # silencieux, n'affiche que les erreurs
printr test                         # ticket de test (styles, accents, QR code)
printr text "Salut !"               # texte libre, puis coupe
echo "depuis stdin" | printr text   # lit l'entrée standard
printr text --no-cut "sans coupe"
printr image photo.jpg             # image réduite à 512 px, tramée (Floyd–Steinberg)
printr image --no-dither logo.png   # simple seuil noir/blanc, pour logos et dessins au trait
```

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
| `date` | — | « Mercredi 7 octobre 2026 » |
| `feed` | `lines` (1) | Espace vertical |
| `image` | `path` ou `url`, `dither` (true) | Réduite à 512 px |
| `qr` | `data`, `size` (6), `caption` | |
| `weather` | `location`, `days` (1, max 7) | [Open-Meteo](https://open-meteo.com), sans clé. `location` : ville ou `"lat,lon"` |
| `horoscope` | `sign`, `tone` (`serieux`/`farfelu`/`vachard`/`insultant`) | Généré par Claude, sur un thème tiré au hasard. Signe en français ou en anglais |
| `saint` | — | Calendrier local, hors ligne |
| `todo` | `items`, `title` (« À faire ») | Cases à cocher |
| `sudoku` | `difficulty` (`facile`/`moyen`/`difficile`), `seed`, `solution` | Le n° imprimé est la graine : même `seed` + `"solution": true` imprime la solution |
| `maze` | `width` (12), `height` (16), `seed` | |
| `word_of_the_day` | — | Choisi par Claude, sans répéter les 60 derniers mots |
| `quote` | — | Citation du jour, liste locale |
| `holidays` | `zone` (`metropole`/`alsace-moselle`), `count` (1) | Calcul local, vérifié contre calendrier.api.gouv.fr |
| `countdown` | `label`, `date` (`AAAA-MM-JJ`) | « J-79 avant : Noël » |
| `moon` | — | Phase, illumination, prochaines pleine et nouvelle lunes. Calcul local |
| `on_this_day` | `count` (3) | Éphéméride, Wikipédia en français |
| `air_quality` | `location` | Indice européen, particules, pollens (Open-Meteo) |
| `crypto` | `coins` (`["bitcoin", "ethereum"]`, identifiants CoinGecko), `currency` (`eur`) | Cours et variation sur 24 h |
| `sun` | `location` | Lever, coucher et durée du jour via Open-Meteo, sans clé |
| `challenge` | `show_answer` (`false`) | Énigme ou question de logique du jour, générée localement |

Les blocs `horoscope` et `word_of_the_day` utilisent l'API Claude : définir `ANTHROPIC_API_KEY`
(et optionnellement `PRINTR_CLAUDE_MODEL`, `claude-opus-5-5` par défaut).

Les contenus dépendant d'une date sont **mis en cache** : réimprimer le même ticket redonne les
mêmes résultats, sans nouvel appel. `--refresh` force une nouvelle génération. Le cache vit dans
`~/.cache/printr` (ou `$PRINTR_CACHE_DIR`, ou `/var/cache/printr` avec le service systemd).
`PRINTR_DEBUG=1` affiche les réponses brutes de l'API.

## Serveur HTTP

```sh
PRINTR_TOKEN=$(openssl rand -hex 24) printr serve --listen 0.0.0.0:8080
```

| Route | Corps | Effet |
| --- | --- | --- |
| `GET /` | — | Vérifie que le serveur tourne |
| `POST /print` | Un ticket JSON | Imprime le ticket |
| `POST /todo` | `{"title"?, "items"?: [...], "text"?: "une\nligne\npar\nélément"}` | Imprime la date et une liste à cocher |

Les `POST` exigent l'en-tête `Authorization: Bearer <jeton>`. Ajouter `?preview` renvoie
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
Les blocs Claude demandent `ANTHROPIC_API_KEY` dans l'environnement.

À la main, dans deux terminaux :

```sh
cd emulator && uvx emupos run                # terminal 1
cargo run -- --tcp 127.0.0.1:9100 test       # terminal 2
# → emulator/receipts/*.png et *.txt
```

## Compiler pour le Raspberry Pi

Avec [`cross`](https://github.com/cross-rs/cross) (nécessite Docker) :

```sh
cross build --release --target aarch64-unknown-linux-gnu    # Pi 3 / Zero 2 W, OS 64 bits
cross build --release --target arm-unknown-linux-gnueabihf  # Pi Zero W (ARMv6)
scp target/aarch64-unknown-linux-gnu/release/printr pi@raspberrypi:
```

## Installation sur le Pi

L'imprimante est exposée par le module noyau `usblp` en `/dev/usb/lp0`.
Pour y accéder sans `sudo` :

```sh
sudo cp deploy/70-tm-t88v.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
sudo usermod -aG lp $USER   # puis se reconnecter
```

La règle crée aussi le lien stable `/dev/tm88` : `printr --device /dev/tm88 test`.

Pour lancer le serveur au démarrage, voir [deploy/printr.service](deploy/printr.service)
et [deploy/printr.env.example](deploy/printr.env.example) (jeton, clé API, périphérique).
