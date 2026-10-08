#!/usr/bin/env bash
# Régénère les illustrations du site et du README, avec des données fictives uniquement :
#   - captures de l'interface web (docs/shots/) ;
#   - tickets imprimés dans l'émulateur (docs/tickets/).
# Prérequis : uv, chromium, et l'émulateur (uvx emupos).
set -euo pipefail
cd "$(dirname "$0")/../.."

PORT_WEB=8097
PORT_PRINTER=9100
DEMO=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$DEMO"' EXIT

cargo build --release -q
BIN=target/release/printr

# Données fictives : trois comptes, quelques tickets enregistrés.
export PRINTR_DATA_DIR="$DEMO/data" PRINTR_CACHE_DIR="$DEMO/cache"
export PRINTR_GLITCH=0  # pas de glitch surprise dans les illustrations
for name in Camille Alex Léo; do echo demo1234 | $BIN user add "$name" >/dev/null 2>&1; done

$BIN --dump "$DEMO/sortie.bin" serve --listen "127.0.0.1:$PORT_WEB" >"$DEMO/serve.log" 2>&1 &
until nc -z 127.0.0.1 $PORT_WEB 2>/dev/null; do sleep 0.2; done

api() { # api <prénom> <méthode> <chemin> [corps]
  local jar="$DEMO/$1.jar"
  if [ ! -f "$jar" ]; then
    local id
    id=$(curl -s "http://127.0.0.1:$PORT_WEB/api/users" | python3 -c "import json,sys; print([u['id'] for u in json.load(sys.stdin) if u['name']=='$1'][0])")
    curl -s -c "$jar" -X POST "http://127.0.0.1:$PORT_WEB/api/login" -H 'Content-Type: application/json' \
      -d "{\"user_id\":\"$id\",\"password\":\"demo1234\"}" >/dev/null
  fi
  curl -s -b "$jar" -X "$2" "http://127.0.0.1:$PORT_WEB$3" -H 'Content-Type: application/json' ${4:+-d "$4"} >/dev/null
}

api Alex POST /api/presets '{"name":"Horoscope du matin","icon":"🔮","ticket":{"blocks":[{"type":"date"},{"type":"horoscope","sign":"lion","tone":"serieux"},{"type":"word_of_the_day"}]},"schedules":[{"days":[1,2,3,4,5],"time":"07:30"}]}'
api Alex POST /api/presets "{\"name\":\"Pause jeux\",\"icon\":\"🧩\",\"ticket\":$(cat docs/demo/tickets/jeux.json)}"
api Camille POST /api/presets '{"name":"Brief du week-end","icon":"☀️","shared":true,"ticket":{"blocks":[{"type":"title","text":"Bon week-end !"},{"type":"weather","location":"Lyon","days":3},{"type":"saint"},{"type":"riddle"}]},"schedules":[{"days":[6,7],"time":"09:00"}]}'
api Léo POST /api/presets '{"name":"Défi du jour","icon":"💪","shared":true,"ticket":{"blocks":[{"type":"workout","level":"difficile"}]},"schedules":[{"days":[1,2,3,4,5,6,7],"time":"18:00"}]}'

echo "Captures de l'interface…"
uv run -q --with playwright python -I docs/demo/capture.py "http://127.0.0.1:$PORT_WEB" docs/shots docs/demo/cafe.jpg

# PNG optimisés : le site et le README restent légers.
uv run -q --with pillow python -I -c "
import glob
from PIL import Image
for f in glob.glob('docs/shots/*.png'):
    Image.open(f).convert('RGB').save(f, optimize=True)
"

echo "Tickets dans l'émulateur…"
(cd emulator && exec uvx emupos run >"$DEMO/emupos.log" 2>&1) &
until nc -z 127.0.0.1 $PORT_PRINTER 2>/dev/null; do sleep 0.3; done
for ticket in docs/demo/tickets/*.json; do
  name=$(basename "$ticket" .json)
  before=$(ls emulator/receipts/*.png 2>/dev/null | wc -l)
  $BIN -q --tcp "127.0.0.1:$PORT_PRINTER" print "$ticket"
  until [ "$(ls emulator/receipts/*.png 2>/dev/null | wc -l)" -gt "$before" ]; do sleep 0.3; done
  cp "$(ls -t emulator/receipts/*.png | head -1)" "docs/tickets/$name.png"
  chmod 644 "docs/tickets/$name.png"
done
pid=$(ss -ltnpH "sport = :$PORT_PRINTER" | grep -o 'pid=[0-9]*' | cut -d= -f2 | head -1)
[ -n "$pid" ] && kill "$pid"

echo "Terminé : docs/shots/ et docs/tickets/"
