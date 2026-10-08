#!/usr/bin/env bash
# Imprime un ticket dans l'émulateur (emupos) et affiche le chemin du rendu PNG.
# Usage : scripts/demo.sh [ticket.json]   (défaut : examples/complet.json)
set -euo pipefail
cd "$(dirname "$0")/.."

ticket="${1:-examples/complet.json}"
port=9100
cargo build -q

listener_pid() { ss -ltnpH "sport = :$port" | grep -o 'pid=[0-9]*' | cut -d= -f2 | head -1; }
if [ -n "$(listener_pid)" ]; then
    echo "le port $port est déjà utilisé (émulateur déjà lancé ?)" >&2
    exit 1
fi

(cd emulator && exec uvx emupos run >/dev/null 2>&1) &
trap 'pid=$(listener_pid); [ -n "$pid" ] && kill "$pid"' EXIT

for _ in $(seq 60); do
    [ -n "$(listener_pid)" ] && break
    sleep 0.5
done

mkdir -p emulator/receipts
# find plutôt que ls : sans aucun PNG, ls échoue et pipefail arrêterait le script.
png_count() { find emulator/receipts -name '*.png' | wc -l; }
before=$(png_count)
./target/debug/printr --tcp "127.0.0.1:$port" print "$ticket"

# L'émulateur termine le ticket après la coupe ; on attend le PNG.
for _ in $(seq 30); do
    [ "$(png_count)" -gt "$before" ] && break
    sleep 0.5
done
ls -t emulator/receipts/*.png | head -1
