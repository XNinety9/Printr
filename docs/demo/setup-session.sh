#!/usr/bin/env bash
# Illustrations de la première mise en route, avec des données fictives uniquement :
#   - captures du choix du Wi-Fi, de l'assistant et des réglages (docs/shots/) ;
#   - tickets du point d'accès et de test, imprimés dans l'émulateur (docs/tickets/).
# Usage : docs/demo/setup-session.sh [--en]   (--en : en anglais, dans docs/shots/en/ et docs/tickets/en/)
# Prérequis : uv, chromium, l'émulateur (uvx emupos), unshare (util-linux) et le module noyau dummy.
#
# Tout tourne dans un espace isolé (unshare, sans sudo) : un réseau fictif (la machine en
# 192.168.1.10, une imprimante « réseau » en 192.168.1.50 qui relaie vers l'émulateur), de faux
# nmcli et systemctl, et Printr sur le port 80, comme sur l'image Raspberry Pi. Le point d'accès
# s'ouvre après la minute d'attente du réseau, comme en vrai.
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ "${1:-}" != --inside ]; then
  cargo build --release -q
  # Le cache de uv sert hors réseau : Playwright et l'émulateur doivent y être déjà (photo-session.sh).
  exec unshare --user --map-root-user --net --mount "$0" --inside "${1:-}"
fi
shift

SHOTS=docs/shots TICKETS=docs/tickets LANGUE=fr
if [ "${1:-}" = "--en" ]; then
  SHOTS=docs/shots/en TICKETS=docs/tickets/en LANGUE=en
fi
mkdir -p "$SHOTS" "$TICKETS"
BIN=$PWD/target/release/printr
DEMO=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$DEMO"' EXIT
export UV_OFFLINE=1 PRINTR_GLITCH=0

# Les réglages de la machine ne doivent pas s'inviter dans les captures.
unset PRINTR_LANG PRINTR_TCP PRINTR_DEVICE PRINTR_CITY PRINTR_CODEPAGE PRINTR_TOKEN PRINTR_MQTT_URL \
  ANTHROPIC_API_KEY INVOCATION_ID PRINTR_DATA_DIR PRINTR_LISTEN

# Réseau fictif, et un /run à soi (Printr y écrit la configuration de hostapd).
mount -t tmpfs tmpfs /run
mkdir -p /run/printr
ip link set lo up
ip link add dummy0 type dummy
ip addr add 192.168.1.10/24 dev dummy0
ip addr add 192.168.1.50/32 dev dummy0
ip link set dummy0 up
ip route add default dev dummy0

# Faux nmcli (des réseaux à portée, une interface Wi-Fi sans connexion) et faux systemctl.
mkdir -p "$DEMO/bin"
if [ "$LANGUE" = en ]; then
  networks='Home Sweet Home:82:WPA2\nThe Neighbors:41:WPA1 WPA2\nCorner Café:23:\n'
else
  networks='La Maison:82:WPA2\nLes Voisins:41:WPA1 WPA2\nCafé du coin:23:\n'
fi
cat >"$DEMO/bin/nmcli" <<EOF
#!/bin/sh
case "\$*" in
  *"device wifi list"*) printf '$networks' ;;
  *"-f DEVICE"*) echo 'wlan0:wifi:disconnected:' ;;
  *"connection show"*) ;;
esac
exit 0
EOF
printf '#!/bin/sh\nexit 0\n' >"$DEMO/bin/systemctl"
chmod +x "$DEMO/bin/nmcli" "$DEMO/bin/systemctl"

# L'émulateur, avec son propre dossier de tickets, et le relais de l'imprimante « réseau ».
cp -r emulator "$DEMO/emu"
rm -rf "$DEMO/emu/receipts"
mkdir "$DEMO/emu/receipts"
(cd "$DEMO/emu" && exec uvx emupos run >"$DEMO/emupos.log" 2>&1) &
until nc -z 127.0.0.1 9100 2>/dev/null; do sleep 0.3; done
cat >"$DEMO/relay.py" <<'EOF'
import socket, threading
def pipe(a, b):
    try:
        while (d := a.recv(65536)):
            b.sendall(d)
    except OSError:
        pass
    for s in (a, b):
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
srv = socket.socket()
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("192.168.1.50", 9100))
srv.listen()
while True:
    c, _ = srv.accept()
    try:
        u = socket.create_connection(("127.0.0.1", 9100))
    except OSError:
        c.close()
        continue
    threading.Thread(target=pipe, args=(c, u), daemon=True).start()
    threading.Thread(target=pipe, args=(u, c), daemon=True).start()
EOF
python3 -I "$DEMO/relay.py" &

# La langue en réglage de l'appli, pas fixée par le système (elle n'apparaît pas verrouillée).
for d in wifi app; do
  mkdir -p "$DEMO/$d"
  printf '{"users":[],"settings":{"PRINTR_LANG":"%s"}}' "$LANGUE" >"$DEMO/$d/printr.json"
done
# Printr sans réseau : il ouvre son point d'accès et imprime le ticket pour s'y connecter.
PATH="$DEMO/bin:$PATH" PRINTR_WIFI_SETUP=1 PRINTR_DATA_DIR="$DEMO/wifi" PRINTR_CACHE_DIR="$DEMO/cache" \
  "$BIN" --tcp 127.0.0.1:9100 serve --listen 127.0.0.1:80 >"$DEMO/wifi.log" 2>&1 &
# Printr neuf, relancé comme par systemd quand l'assistant enregistre les réglages.
(
  while true; do
    INVOCATION_ID=demo PRINTR_DATA_DIR="$DEMO/app" PRINTR_CACHE_DIR="$DEMO/cache" \
      "$BIN" serve --listen 192.168.1.10:80 >>"$DEMO/app.log" 2>&1 || [ $? = 75 ] || break
  done
) &
until nc -z 192.168.1.10 80 2>/dev/null; do sleep 0.2; done

echo "Captures de la première mise en route…"
DEMO_LANG=$LANGUE uv run -q --with playwright python -I docs/demo/capture-setup.py "$SHOTS"
uv run -q --with pillow python -I -c "
from PIL import Image
for name in ('mobile-wifi', 'mobile-wizard', 'mobile-settings'):
    f = '$SHOTS/' + name + '.png'
    Image.open(f).convert('RGB').save(f, optimize=True)
"

# Les tickets reçus par l'émulateur : celui du point d'accès et celui de l'assistant.
for txt in "$DEMO"/emu/receipts/*.txt; do
  case "$(cat "$txt")" in
    *Printr-*) name=wifi ;;
    *12,50*) name=test ;;
    *) continue ;;
  esac
  cp "${txt%.txt}.png" "$TICKETS/$name.png"
  chmod 644 "$TICKETS/$name.png"
  echo "$TICKETS/$name.png"
done
