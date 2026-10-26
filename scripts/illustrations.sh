#!/usr/bin/env bash
# Refait toutes les illustrations du site et du README, dans les deux langues, puis construit le
# site : à lancer avant chaque version.
#   1. tickets d'exemple des blocs (docs/exemples/, docs/exemples/en/) ;
#   2. captures de l'appli et tickets vitrines (docs/shots/, docs/tickets/, et leurs en/), dont la
#      première mise en route : point d'accès, assistant, réglages ;
#   3. image de partage (docs/preview.png) ;
#   4. site (_site/).
# Usage : scripts/illustrations.sh [--sans-claude] [--seulement exemples|captures|partage|site]
#   --sans-claude : aucun appel à Claude ; les blocs qui en ont besoin gardent leur image actuelle.
# Prérequis : uv, chromium, l'émulateur (uvx emupos) et unshare. Barnum : $PRINTR_BARNUM, sinon ../Barnum.
set -euo pipefail
cd "$(dirname "$0")/.."

NO_CLAUDE=0 ONLY=''
while [ $# -gt 0 ]; do
  case "$1" in
    --sans-claude) NO_CLAUDE=1 ;;
    --seulement) ONLY=$2; shift ;;
    -h | --help) sed -n '2,10s/^# \{0,1\}//p' "$0"; exit 0 ;;
    *) echo "option inconnue : $1 (voir --help)" >&2; exit 1 ;;
  esac
  shift
done

if [ -t 1 ]; then
  B=$'\e[1m' D=$'\e[2m' R=$'\e[0m' GREEN=$'\e[32m' RED=$'\e[31m' ORANGE=$'\e[38;5;208m'
else
  B='' D='' R='' GREEN='' RED='' ORANGE=''
fi
LOGS=$(mktemp -d)
trap 'rm -rf "$LOGS"' EXIT
START=$SECONDS

# Une étape : son nom, le nombre d'images attendues (0 si on ne compte pas), sa commande. Le détail
# va dans un journal, montré seulement en cas d'échec ; en attendant, la ligne de l'étape montre la
# progression : chaque image produite est annoncée par une ligne « …/nom.png » dans le journal.
run() {
  local name=$1 total=$2 log t=$SECONDS
  shift 2
  log="$LOGS/$(echo "$name" | tr -c 'a-zA-Z0-9' '_').log"
  "$@" >"$log" 2>&1 &
  local pid=$! frame=0 spin=(⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏)
  if [ -t 1 ]; then
    while kill -0 "$pid" 2>/dev/null; do
      local mark="$ORANGE${spin[frame++ % ${#spin[@]}]}$R" elapsed="$D$((SECONDS - t))s$R"
      if [ "$total" -gt 0 ]; then
        local n last bar
        n=$(grep -c '\.png$' "$log" || true)
        last=$(grep '\.png$' "$log" | tail -1 || true)
        last=${last##*/}
        last=${last%.png}
        bar=''
        for ((k = 0; k < 20; k++)); do
          if ((k < n * 20 / total)); then bar+='█'; else bar+='░'; fi
        done
        # Avant la première image : compilation, démarrage de l'émulateur ou du navigateur.
        [ "$n" = 0 ] && last="préparation…"
        printf '\r\e[K%s %s  %s %s%d/%d%s %s %s' "$mark" "$name" "$bar" "$B" "$n" "$total" "$R" "$elapsed" "${last:+$D· $last$R}"
      else
        printf '\r\e[K%s %s  %s' "$mark" "$name" "$elapsed"
      fi
      sleep 0.2
    done
    printf '\r\e[K'
  fi
  if wait "$pid"; then
    printf '%s▸%s %s %s✓%s %s(%ss)%s\n' "$B$ORANGE" "$R" "$name" "$GREEN" "$R" "$D" "$((SECONDS - t))" "$R"
  else
    printf '%s✗ %s a échoué%s, fin du journal :\n' "$RED$B" "$name" "$R" >&2
    tail -20 "$log" >&2
    exit 1
  fi
}
wanted() { [ -z "$ONLY" ] || [ "$ONLY" = "$1" ]; }

# Barnum (horoscope hors ligne) : sans lui, son exemple serait un message d'erreur.
if [ -z "${PRINTR_BARNUM:-}" ] && ! command -v barnum >/dev/null && [ -f ../Barnum/main.py ]; then
  PRINTR_BARNUM="python3 $(cd ../Barnum && pwd)/main.py"
  export PRINTR_BARNUM
fi
export CROSSWORDS_CACHE="${CROSSWORDS_CACHE:-$HOME/.cache/crosswords}"

printf '\n%s🖼️  Illustrations de Printr%s\n\n' "$B" "$R"

if wanted exemples; then
  # Les blocs à exemple : tout le catalogue du site ; sans Claude, on écarte ceux qui en ont besoin.
  mapfile -t blocks < <(python3 -I -c '
import json, sys
catalogue = json.load(open("docs/exemples/catalogue.json"))
skip = sys.argv[1] == "1"
for group in catalogue:
    for block in group["blocs"]:
        if not (skip and block.get("claude")):
            print(block["id"])
' "$NO_CLAUDE")
  # Hors catalogue : la démonstration des tramages, montrée dans le guide.
  blocks+=(tramages)
  if [ "$NO_CLAUDE" = 1 ]; then
    printf '  %ssans Claude : les exemples des blocs Claude gardent leur image actuelle%s\n' "$D" "$R"
  fi
  exemples() {
    if [ "$NO_CLAUDE" = 1 ]; then env -u ANTHROPIC_API_KEY docs/exemples/generer.sh "$@"; else docs/exemples/generer.sh "$@"; fi
  }
  run "Exemples des blocs, en français" "${#blocks[@]}" exemples "${blocks[@]}"
  run "Exemples des blocs, en anglais" "${#blocks[@]}" exemples --en "${blocks[@]}"
fi

if wanted captures; then
  # Images attendues : chaque capture (celles de l'ordinateur deux fois, thème clair et sombre),
  # plus les tickets vitrines.
  shots=$(grep -c 'await snap(' docs/demo/capture.py)
  desktop=$(grep -c 'await snap(page, f"{OUT}/desktop' docs/demo/capture.py)
  tickets=$(find docs/demo/tickets -maxdepth 1 -name '*.json' | wc -l)
  run "Captures de l'appli et tickets vitrines, en français" $((shots + desktop + tickets)) docs/demo/photo-session.sh
  tickets=$(find docs/demo/tickets/en -maxdepth 1 -name '*.json' | wc -l)
  run "Captures de l'appli et tickets vitrines, en anglais" $((shots + desktop + tickets)) docs/demo/photo-session.sh --en
  # Première mise en route (réseau isolé, point d'accès après une minute) : trois captures, deux tickets.
  run "Première mise en route, en français" 5 docs/demo/setup-session.sh
  run "Première mise en route, en anglais" 5 docs/demo/setup-session.sh --en
fi

if wanted site || wanted partage; then
  run "Construction du site" 0 uvx -q --with markdown --with pymdown-extensions python docs/build.py
fi

if wanted partage; then
  # L'image de partage se prend sur le site construit, servi un instant en local.
  partage() {
    local port=8321
    python3 -m http.server -d _site "$port" >/dev/null 2>&1 &
    local server=$!
    until curl -s -o /dev/null "http://127.0.0.1:$port/"; do sleep 0.2; done
    local status=0
    uv run -q --with playwright --with pillow python -I docs/demo/partage.py "http://127.0.0.1:$port/" "$LOGS/partage.png" || status=$?
    kill "$server"
    [ "$status" = 0 ] || return "$status"
    uv run -q --with pillow python -I -c "
import sys
from PIL import Image
Image.open(sys.argv[1]).convert('RGB').save(sys.argv[2], optimize=True)" "$LOGS/partage.png" docs/preview.png
  }
  run "Image de partage (docs/preview.png)" 0 partage
  # Le site doit embarquer la nouvelle image.
  run "Construction du site, avec l'image" 0 uvx -q --with markdown --with pymdown-extensions python docs/build.py
fi

changed=$(git status --short -- docs | wc -l)
printf '\n  %s✓ Terminé en %ss%s · %s fichier(s) modifié(s) dans docs/ · site dans _site/\n' \
  "$GREEN$B" "$((SECONDS - START))" "$R" "$changed"
printf '  %sà relire avant de commiter : git status docs/%s\n\n' "$D" "$R"
