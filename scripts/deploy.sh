#!/usr/bin/env bash
# Déploie printr sur une machine Linux (Raspberry Pi, serveur…), depuis le poste de développement.
#   scripts/deploy.sh HÔTE [--port N] [--install]   machine distante (SSH par clé), ex. pi@printer.local
#   scripts/deploy.sh --local                       cette machine
# Première fois : compile pour la bonne architecture, envoie et lance deploy/install.sh (le mot
# de passe sudo est demandé une fois). Ensuite : remplace le binaire et redémarre le service,
# sans sudo. --install relance l'installation complète (elle ne perd rien : voir install.sh).
# Prérequis pour une machine distante : cross (et Docker) pour compiler contre une glibc ancienne.
set -euo pipefail
cd "$(dirname "$0")/.."

HOST='' PORT=22 FULL=0 LOCAL=0
while [ $# -gt 0 ]; do
  case "$1" in
    --port) PORT=$2; shift ;;
    --install) FULL=1 ;;
    --local) LOCAL=1 ;;
    -h | --help) sed -n '2,9s/^# \{0,1\}//p' "$0"; exit 0 ;;
    -*) echo "option inconnue : $1 (voir --help)" >&2; exit 1 ;;
    *) HOST=$1 ;;
  esac
  shift
done

if [ -t 1 ]; then
  B=$'\e[1m' D=$'\e[2m' R=$'\e[0m' GREEN=$'\e[32m' RED=$'\e[31m' CYAN=$'\e[36m' ORANGE=$'\e[38;5;208m'
else
  B='' D='' R='' GREEN='' RED='' CYAN='' ORANGE=''
fi
step() { printf '%s▸%s %s' "$B$ORANGE" "$R" "$1"; }
done_() { printf ' %s✓%s %s\n' "$GREEN" "$R" "${1:-}"; }
fail() { printf '\n%s✗ %s%s\n' "$RED$B" "$1" "$R" >&2; exit 1; }

# ---- Installation sur cette machine ----
if [ "$LOCAL" = 1 ]; then
  printf '\n%s🧾 printr → cette machine%s\n\n' "$B" "$R"
  step "Compilation…"
  cargo build --release -q || fail "compilation échouée"
  done_
  exec sudo bash deploy/install.sh --binary "$PWD/target/release/printr"
fi

[ -n "$HOST" ] || { sed -n '2,9s/^# \{0,1\}//p' "$0"; exit 1; }
SSH=(ssh -o BatchMode=yes -o ConnectTimeout=8 -p "$PORT")
SCP=(scp -q -o BatchMode=yes -P "$PORT")
printf '\n%s🧾 printr → %s%s\n\n' "$B" "$HOST" "$R"

step "Connexion…"
"${SSH[@]}" "$HOST" true 2>/dev/null || fail "connexion SSH impossible à $HOST (machine allumée ? clé SSH installée ?)"
arch=$("${SSH[@]}" "$HOST" uname -m)
case "$arch" in
  aarch64 | arm64) TARGET=aarch64-unknown-linux-gnu ;;
  armv7l) TARGET=armv7-unknown-linux-gnueabihf ;;
  armv6l) TARGET=arm-unknown-linux-gnueabihf ;;
  x86_64) TARGET=x86_64-unknown-linux-gnu ;;
  *) fail "architecture non prise en charge : $arch" ;;
esac
done_ "$arch · $("${SSH[@]}" "$HOST" '. /etc/os-release 2>/dev/null && echo "$PRETTY_NAME" || uname -s')"

step "Compilation pour $TARGET…"
log=$(mktemp)
if command -v cross >/dev/null; then
  cross build --release --target "$TARGET" >"$log" 2>&1 || { cat "$log" >&2; fail "compilation échouée"; }
else
  # Sans cross, seule une cible identique à cette machine est possible (glibc récente : risqué).
  [ "$arch" = "$(uname -m)" ] || fail "cross est nécessaire pour compiler vers $arch (cargo install cross)"
  cargo build --release --target "$TARGET" >"$log" 2>&1 || { cat "$log" >&2; fail "compilation échouée"; }
fi
rm -f "$log"
mkdir -p "target/deploy/$TARGET"
cp "target/$TARGET/release/printr" "target/deploy/$TARGET/printr"
llvm-strip "target/deploy/$TARGET/printr" 2>/dev/null || strip "target/deploy/$TARGET/printr" 2>/dev/null || true
BIN=target/deploy/$TARGET/printr
done_ "$(du -h "$BIN" | cut -f1)"

# Déjà installé et administrable sans sudo ? (groupe printr actif et dossier du binaire accessible)
installed=$("${SSH[@]}" "$HOST" '[ -w /opt/printr ] && systemctl cat printr >/dev/null 2>&1 && echo oui || echo non')

if [ "$FULL" = 1 ] || [ "$installed" != oui ]; then
  step "Envoi des fichiers d'installation…"
  "${SSH[@]}" "$HOST" 'rm -rf ~/printr-install && mkdir -p ~/printr-install'
  "${SCP[@]}" "$BIN" deploy/install.sh deploy/printr.service deploy/printr.env.example \
    deploy/70-tm-t88v.rules deploy/50-printr.rules "$HOST:printr-install/"
  done_
  printf '%s  Installation : la machine va demander le mot de passe sudo (une seule fois).%s\n' "$D" "$R"
  ssh -t -p "$PORT" "$HOST" 'sudo bash ~/printr-install/install.sh && rm -rf ~/printr-install'
else
  step "Mise à jour du programme…"
  "${SCP[@]}" "$BIN" "$HOST:/opt/printr/printr.new"
  "${SSH[@]}" "$HOST" 'cp -p /opt/printr/printr /opt/printr/printr.old; chmod 775 /opt/printr/printr.new; mv /opt/printr/printr.new /opt/printr/printr'
  done_ "$("${SSH[@]}" "$HOST" /opt/printr/printr --version)"
  step "Redémarrage du service…"
  "${SSH[@]}" "$HOST" 'systemctl restart printr' || fail "redémarrage refusé (règle polkit absente ? relance avec --install)"
  sleep 2
  "${SSH[@]}" "$HOST" 'systemctl is-active -q printr' || {
    "${SSH[@]}" "$HOST" 'journalctl -u printr -n 10 --no-pager -o cat' >&2
    printf '%s  Version précédente : /opt/printr/printr.old%s\n' "$D" "$R" >&2
    fail "le service ne démarre pas (journal ci-dessus)"
  }
  done_
  port=$("${SSH[@]}" "$HOST" "sed -n 's/^PRINTR_LISTEN=.*:\([0-9]*\)\$/\1/p' /etc/printr.env | tail -1")
  printf '\n%s  À jour !%s %shttp://%s:%s%s\n\n' "$B$GREEN" "$R" "$CYAN" "${HOST#*@}" "${port:-8080}" "$R"
fi
