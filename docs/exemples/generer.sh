#!/usr/bin/env bash
# Régénère les exemples de blocs (docs/exemples/*.png) dans l'émulateur, avec des données
# fictives uniquement (liste de courses et historique de Camille, Alex et Léo).
# Usage : docs/exemples/generer.sh [bloc…]   (défaut : tous les .json du dossier)
# Prérequis : uv et l'émulateur (uvx emupos).
set -euo pipefail
cd "$(dirname "$0")/../.."

PORT=9100
DEMO=$(mktemp -d)
stop_emulator() { ss -ltnpH "sport = :$PORT" | grep -o 'pid=[0-9]*' | cut -d= -f2 | xargs -r kill; }
trap 'stop_emulator; kill $(jobs -p) 2>/dev/null; rm -rf "$DEMO"' EXIT

cargo build --release -q
BIN=target/release/printr
export PRINTR_DATA_DIR="$DEMO/data" PRINTR_CACHE_DIR="$DEMO/cache" PRINTR_GLITCH=0
mkdir -p "$PRINTR_DATA_DIR"

# Données fictives : une liste de courses et un mois d'impressions.
python3 -I - "$PRINTR_DATA_DIR/printr.json" <<'EOF'
import json, random, sys
from datetime import datetime, timedelta, timezone

rng = random.Random(42)
now = datetime.now(timezone.utc)
shopping = [
    ("Lait", "Camille"), ("Pain de mie", "Léo"), ("Pommes", "Camille"), ("Café", "Alex"),
    ("Céréales au chocolat", "Léo"), ("Lessive", "Alex"), ("Tomates", "Camille"), ("Rouleaux de papier thermique", "Alex"),
]
people = ["Camille"] * 9 + ["Alex"] * 6 + ["Léo"] * 5 + ["planification"] * 7
tickets = [["date", "météo", "saint du jour", "citation"], ["sudoku"], ["énigme", "défi sportif"],
           ["horoscope Barnum"], ["image", "texte"], ["mots mêlés"], ["liste de courses"], ["sudoku", "énigme"]]
first = now.replace(day=1, hour=0, minute=0, second=0, microsecond=0)
history = []
for _ in range(40):
    at = first + timedelta(minutes=rng.randrange(max(1, int((now - first).total_seconds() // 60))))
    if rng.random() < 0.4:
        at = at.replace(hour=7, minute=30)
    history.append({
        "at": at.isoformat(), "by": rng.choice(people), "label": "Ticket", "ok": rng.random() > 0.05,
        "blocks": rng.choice(tickets), "paper_mm": rng.randrange(90, 320),
    })
history.sort(key=lambda e: e["at"])
data = {
    "shopping": [{"id": f"x{i}", "text": t, "by": b, "added": now.isoformat()} for i, (t, b) in enumerate(shopping)],
    "history": history,
}
json.dump(data, open(sys.argv[1], "w"), ensure_ascii=False, indent=1)
EOF

if nc -z 127.0.0.1 $PORT 2>/dev/null; then echo "le port $PORT est déjà utilisé" >&2; exit 1; fi
(cd emulator && exec uvx emupos run >"$DEMO/emupos.log" 2>&1) &
until nc -z 127.0.0.1 $PORT 2>/dev/null; do sleep 0.3; done
mkdir -p emulator/receipts

names=("$@")
[ ${#names[@]} -eq 0 ] && names=($(ls docs/exemples/*.json | xargs -n1 basename | sed 's/\.json$//'))
for name in "${names[@]}"; do
  before=$(find emulator/receipts -name '*.png' | wc -l)
  $BIN -q --tcp "127.0.0.1:$PORT" print "docs/exemples/$name.json"
  until [ "$(find emulator/receipts -name '*.png' | wc -l)" -gt "$before" ]; do sleep 0.3; done
  cp "$(ls -t emulator/receipts/*.png | head -1)" "docs/exemples/$name.png"
  chmod 644 "docs/exemples/$name.png"
  echo "docs/exemples/$name.png"
done
