#!/usr/bin/env bash
# Laboratoire de la mise à jour automatique, entièrement en local : ni GitHub, ni Pi.
#
#   scripts/labo-maj.sh              les scénarios s'enchaînent et sont vérifiés
#   scripts/labo-maj.sh --pas-a-pas  démonstration guidée : explications, pauses, état en direct
#
# Un faux dépôt GitHub sert des releases ; printr tourne comme sous systemd (script de garde avant
# chaque démarrage, relance quand il s'arrête) en mode automatique, avec une clé de test jetable.
#   1. une bonne version             → installée, confirmée
#   2. une version qui plante        → installée, puis retour automatique à la précédente, écartée
#   3. une signature d'une autre clé → refusée, rien ne change
#   4. un binaire corrompu           → refusé (empreinte), rien ne change
#   5. une nouvelle bonne version    → installée : le système s'en remet
#
# Les compilations sont gardées dans $LAB (défaut /tmp/printr-labo) : une relance est rapide.
set -euo pipefail
cd "$(dirname "$0")/.."
# Le journal est lu en français ; et pas de faux systemd hérité de la session.
unset PRINTR_LANG INVOCATION_ID

GUIDED=0
[ "${1:-}" = "--pas-a-pas" ] && GUIDED=1
LAB=${LAB:-/tmp/printr-labo}
PORT_WEB=8310
PORT_DEPOT=8311
REPO=XNinety9/Printr
ARCH=$(uname -m)

if [ -t 1 ]; then
  B=$'\e[1m' D=$'\e[2m' R=$'\e[0m' GREEN=$'\e[32m' RED=$'\e[31m' ORANGE=$'\e[38;5;208m' CYAN=$'\e[36m' YELLOW=$'\e[33m'
else
  B='' D='' R='' GREEN='' RED='' ORANGE='' CYAN='' YELLOW=''
fi
step() { printf '\n%s━━ %s%s\n' "$B$ORANGE" "$1" "$R"; }
ok() { printf '  %s✓%s %s\n' "$GREEN" "$R" "$1"; }
info() { printf '    %s%s%s\n' "$D" "$1" "$R"; }
fail() { printf '\n%s✗ %s%s\n  journal : %s\n' "$RED$B" "$1" "$R" "$LAB/journal.log" >&2; exit 1; }
# Mode guidé seulement : explications, fichiers montrés, pauses.
say() { [ "$GUIDED" = 1 ] || return 0; printf '%s\n' "$@" | sed "s/^/  /"; }
show() { # show TITRE FICHIER
  [ "$GUIDED" = 1 ] || return 0
  printf '  %s┌ %s%s\n' "$CYAN" "$1" "$R"
  sed "s/^/  $CYAN│$R /" "$2"
  printf '  %s└%s\n' "$CYAN" "$R"
}
pause() { [ "$GUIDED" = 1 ] || return 0; printf '\n  %s⏎ Entrée pour continuer…%s' "$D" "$R"; read -r _ < /dev/tty; echo; }

if ss -ltnH "sport = :$PORT_WEB or sport = :$PORT_DEPOT" 2>/dev/null | grep -q .; then
  fail "les ports $PORT_WEB ou $PORT_DEPOT sont occupés (un laboratoire tourne déjà ?)"
fi
cleanup() {
  touch "$LAB/stop" 2>/dev/null || true
  for port in $PORT_WEB $PORT_DEPOT; do
    ss -ltnpH "sport = :$port" 2>/dev/null | grep -o 'pid=[0-9]*' | cut -d= -f2 | xargs -r kill 2>/dev/null || true
  done
  jobs -p | xargs -r kill 2>/dev/null || true
  wait 2>/dev/null || true
}
trap cleanup EXIT

# On garde les compilations ; tout le reste repart de zéro.
mkdir -p "$LAB"
rm -rf "$LAB"/{opt,data,cache,depot,cles,autre-cle,stop}
mkdir -p "$LAB"/{opt,data,cache,depot,versions}
: > "$LAB/journal.log"

printf '\n%s🧪 Laboratoire de la mise à jour automatique%s %s(%s)%s\n' "$B" "$R" "$D" "$LAB" "$R"
say "" "Tout se passe sur cette machine : un faux GitHub, un faux systemd, des releases piégées." \
    "Ni ton dépôt, ni ton Pi ne sont touchés."

# ---- Préparation ----
step "Les clés"
export CARGO_TARGET_DIR="$LAB/target"
SIGNER_PASSWORD='' cargo run -q --example signer -- keygen "$LAB/cles" > "$LAB/cle.txt" 2>/dev/null
SIGNER_PASSWORD='' cargo run -q --example signer -- keygen "$LAB/autre-cle" > /dev/null 2>&1
KEY=$(sed -n 's/^Clé publique[^:]*: //p' "$LAB/cle.txt")
ok "une clé de test jetable, et une autre clé pour jouer le faussaire"
say "" "La clé publique, que printr connaît (en vrai, elle sera compilée dans le programme) :"
show "printr.pub" "$LAB/cles/printr.pub"
say "La clé privée, elle, ne sort jamais de la machine qui signe (droits $(stat -c %A "$LAB/cles/printr.key"))."
pause

step "Les versions de test"
rsync -a --delete --exclude target --exclude .git --exclude emulator/receipts ./ "$LAB/src/"
build() { # build VERSION : compile printr sous ce numéro, sauf si les sources n'ont pas bougé
  local out="$LAB/versions/printr-$1"
  if [ -x "$out" ] && [ -z "$(find "$LAB/src/src" "$LAB/src/web" "$LAB/src/Cargo.toml" -newer "$out" -print -quit)" ]; then
    ok "printr $1 (déjà compilée)"
    return
  fi
  info "compilation de printr $1 (la première fois prend quelques minutes)…"
  sed -i "0,/^version = \".*\"/s//version = \"$1\"/" "$LAB/src/Cargo.toml"
  (cd "$LAB/src" && cargo build --release -q 2>/dev/null) || fail "compilation de $1 impossible"
  cp "$CARGO_TARGET_DIR/release/printr" "$out"
  touch "$out"
  ok "printr $1"
}
build 2.0.0
build 2.0.1
build 2.0.5
# La version qui plante : elle annonce bien son numéro, mais s'arrête aussitôt lancée.
printf '#!/bin/sh\n[ "${1:-}" = "--version" ] && { echo "printr 2.0.2"; exit 0; }\necho "printr 2.0.2 : erreur fatale au démarrage" >&2\nexit 1\n' > "$LAB/versions/printr-2.0.2"
chmod +x "$LAB/versions/printr-2.0.2"
ok "printr 2.0.2, piégée : elle plantera au démarrage"

# ---- Faux dépôt GitHub ----
publish() { # publish VERSION [clé] [corrompre] [binaire] : met une release en ligne comme « latest »
  local version=$1 key=${2:-$LAB/cles/printr.key} corrupt=${3:-} dir="$LAB/depot/download/v$1"
  mkdir -p "$dir" "$LAB/depot/repos/$REPO/releases"
  cp "$LAB/versions/printr-${4:-$version}" "$dir/printr-$ARCH"
  (cd "$dir" && sha256sum "printr-$ARCH" > SHA256SUMS)
  SIGNER_PASSWORD='' cargo run -q --example signer -- sign "$key" "$dir/SHA256SUMS" > /dev/null 2>&1
  # Le faussaire modifie le binaire après coup : l'empreinte ne correspond plus.
  [ -n "$corrupt" ] && printf 'charge malveillante' >> "$dir/printr-$ARCH"
  local url="http://127.0.0.1:$PORT_DEPOT/download/v$version"
  cat > "$LAB/depot/repos/$REPO/releases/latest" <<EOF
{"tag_name": "v$version", "html_url": "$url", "draft": false, "prerelease": false, "assets": [
  {"name": "printr-$ARCH", "browser_download_url": "$url/printr-$ARCH"},
  {"name": "SHA256SUMS", "browser_download_url": "$url/SHA256SUMS"},
  {"name": "SHA256SUMS.minisig", "browser_download_url": "$url/SHA256SUMS.minisig"}]}
EOF
}
(cd "$LAB/depot" && exec python3 -m http.server $PORT_DEPOT --bind 127.0.0.1 >/dev/null 2>&1) &

# ---- printr, comme sous systemd ----
cp "$LAB/versions/printr-2.0.0" "$LAB/opt/printr"
echo laboratoire1 | PRINTR_DATA_DIR="$LAB/data" "$LAB/opt/printr" -q user add Labo > /dev/null
supervise() {
  while [ ! -f "$LAB/stop" ]; do
    STATE_DIRECTORY="$LAB/data" PRINTR_BIN="$LAB/opt/printr" sh deploy/printr-update-guard.sh >> "$LAB/journal.log" 2>&1 || true
    set +e
    PRINTR_DATA_DIR="$LAB/data" STATE_DIRECTORY="$LAB/data" PRINTR_CACHE_DIR="$LAB/cache" PRINTR_GLITCH=0 \
      PRINTR_AUTO_UPDATE=auto PRINTR_UPDATE_URL="http://127.0.0.1:$PORT_DEPOT/repos/$REPO" PRINTR_UPDATE_KEY="$KEY" \
      PRINTR_UPDATE_INTERVAL=5s PRINTR_UPDATE_WINDOW=toujours \
      "$LAB/opt/printr" --dump "$LAB/sortie.bin" serve --listen 127.0.0.1:$PORT_WEB >> "$LAB/journal.log" 2>&1
    echo "[systemd] printr s'est arrêté (code $?) : relance" >> "$LAB/journal.log"
    set -e
    sleep 2
  done
}
supervise &

version() { "$LAB/opt/printr" --version 2>/dev/null | awk '{ print $2 }'; }
logged() { grep -q "$1" "$LAB/journal.log"; }
running() { [ "$(version)" = "$1" ] && curl -fs -o /dev/null "http://127.0.0.1:$PORT_WEB/"; }
confirmed() { [ ! -f "$LAB/data/update-pending" ] && logged "mise à jour confirmée : printr .* → $1"; }

# Attend qu'une condition soit vraie. En mode guidé, montre en direct chaque changement d'état
# de printr et chaque nouvelle ligne de son journal.
observe() { # observe SECONDES DESCRIPTION COMMANDE…
  local limit=$1 what=$2; shift 2
  local seen; seen=$(wc -l < "$LAB/journal.log") prev=""
  for i in $(seq "$limit"); do
    if [ "$GUIDED" = 1 ]; then
      local now
      now=$(wc -l < "$LAB/journal.log")
      if [ "$now" -gt "$seen" ]; then
        sed -n "$((seen + 1)),${now}p" "$LAB/journal.log" \
          | grep -E "mise à jour|nouvelle version|retour à|refusée|incorrecte|systemd|erreur fatale|serveur prêt" \
          | grep -v "injoignable: http status: 404" | sed "s/^/    $D│ journal │$R /" || true
        seen=$now
      fi
      local state
      state="installé : $(version || echo '?') · $(curl -fs -o /dev/null "http://127.0.0.1:$PORT_WEB/" && echo "${GREEN}répond${R}" || echo "${YELLOW}silencieux${R}")"
      [ -f "$LAB/data/update-pending" ] && state="$state · ${CYAN}mise à jour en cours ($(tr '\n' ' ' < "$LAB/data/update-pending"))${R}"
      [ -f "$LAB/data/update-pending.starts" ] && state="$state · démarrages notés : $(cat "$LAB/data/update-pending.starts")"
      [ "$state" != "$prev" ] && printf '  %s%3ss%s  %s\n' "$D" "$i" "$R" "$state"
      prev=$state
    fi
    "$@" && return 0
    sleep 1
  done
  # Sans description, on observait seulement, sans rien attendre de précis.
  [ -z "$what" ] && return 0
  fail "délai dépassé : $what"
}

step "Départ"
say "printr 2.0.0 démarre, comme sous systemd, en mode automatique." \
    "Il interroge le faux GitHub toutes les 5 s : aucune release n'est encore publiée."
observe 30 "démarrage de printr 2.0.0" running 2.0.0
ok "printr 2.0.0 tourne, sans release publiée"
pause

step "Scénario 1 · une bonne version (2.0.1)"
publish 2.0.1
say "La release 2.0.1 est en ligne. Voici ce que printr va télécharger :"
show "SHA256SUMS : l'empreinte du binaire (un octet changé, et elle change du tout au tout)" "$LAB/depot/download/v2.0.1/SHA256SUMS"
show "SHA256SUMS.minisig : la signature de cette liste (« RUTr… » rappelle la clé « RWTr… »)" "$LAB/depot/download/v2.0.1/SHA256SUMS.minisig"
say "" "Attendu : signature vérifiée, empreinte vérifiée, binaire essayé (--version)," \
    "remplacement (l'ancien garde sa place dans printr.old), arrêt avec le code 75," \
    "relance par systemd, puis confirmation au bout de 20 s, quand la 2.0.1 répond."
pause
observe 90 "confirmation de 2.0.1" confirmed 2.0.1
running 2.0.1 || fail "2.0.1 ne répond pas"
ok "printr 2.0.1 installée et confirmée ; printr.old garde la $("$LAB/opt/printr.old" --version)"
pause

step "Scénario 2 · une version qui plante au démarrage (2.0.2)"
publish 2.0.2
say "La 2.0.2 est piégée. Pour les vérifications, elle est irréprochable : bien signée," \
    "bonne empreinte, bon numéro. Mais une fois lancée pour de bon, elle s'arrête aussitôt :"
show "printr-$ARCH de la release 2.0.2 (un script qui se fait passer pour printr)" "$LAB/versions/printr-2.0.2"
say "Attendu : installation, plantage, puis le script de garde remet la 2.0.1 au démarrage" \
    "suivant, et écarte la 2.0.2 pour de bon."
pause
observe 90 "retour à 2.0.1" bash -c "grep -q 'retour à' '$LAB/journal.log' && curl -fs -o /dev/null http://127.0.0.1:$PORT_WEB/"
grep -qx "2.0.2" "$LAB/data/update-refused" || fail "2.0.2 n'est pas écartée"
ok "retour automatique à la 2.0.1"
show "update-refused : les versions écartées" "$LAB/data/update-refused"
say "printr continue de vérifier toutes les 5 s : la 2.0.2 est toujours « la dernière release »," \
    "mais il ne la réinstalle pas."
observe 12 "" false
running 2.0.1 && ! [ -f "$LAB/data/update-pending" ] || fail "2.0.2 a été réessayée"
ok "la 2.0.2 n'est pas réessayée"
pause

step "Scénario 3 · une release signée par une autre clé (2.0.3)"
publish 2.0.3 "$LAB/autre-cle/printr.key" "" 2.0.5
say "Un faussaire (compte GitHub piraté, par exemple) publie une 2.0.3, mais il n'a pas la clé :" \
    "il signe avec la sienne. L'identifiant de clé dans sa signature trahit l'imposture :"
show "SHA256SUMS.minisig de la 2.0.3" "$LAB/depot/download/v2.0.3/SHA256SUMS.minisig"
say "Attendu : signature refusée, rien n'est installé."
pause
observe 30 "refus de la signature" logged "signature refusée"
running 2.0.1 || fail "la version a changé"
ok "signature refusée : la 2.0.1 tourne toujours"
pause

step "Scénario 4 · un binaire modifié après signature (2.0.4)"
publish 2.0.4 "" corrompu 2.0.5
say "Cette fois, le faussaire garde une liste d'empreintes correctement signée, mais remplace" \
    "le binaire par un autre. L'empreinte du binaire téléchargé ne correspondra plus :"
show "SHA256SUMS signée (empreinte attendue)" "$LAB/depot/download/v2.0.4/SHA256SUMS"
say "empreinte réelle du binaire : $(sha256sum "$LAB/depot/download/v2.0.4/printr-$ARCH" | cut -d' ' -f1)" \
    "Attendu : empreinte incorrecte, rien n'est installé."
pause
observe 30 "refus de l'empreinte" logged "empreinte de printr-$ARCH incorrecte"
running 2.0.1 || fail "la version a changé"
ok "empreinte incorrecte : la 2.0.1 tourne toujours"
pause

step "Scénario 5 · une nouvelle bonne version (2.0.5)"
publish 2.0.5
say "Une vraie version, bien signée. Attendu : installée et confirmée, malgré les incidents."
pause
observe 90 "confirmation de 2.0.5" confirmed 2.0.5
running 2.0.5 || fail "2.0.5 ne répond pas"
ok "printr 2.0.5 installée et confirmée"

printf '\n%s  Tous les scénarios se sont déroulés comme prévu.%s\n' "$B$GREEN" "$R"
info "journal complet : $LAB/journal.log"
echo
