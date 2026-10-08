#!/usr/bin/env bash
# Installe ou met à jour printr comme service systemd, sur n'importe quel Linux (Raspberry Pi,
# serveur, PC…). À lancer avec sudo, une seule fois :
#   sudo bash install.sh [--binary CHEMIN] [--user COMPTE] [--yes]
#     --binary  le programme à installer (défaut : ./printr à côté du script, sinon
#               target/release/printr dans le dépôt)
#     --user    le compte qui administrera printr sans sudo (défaut : celui qui lance sudo)
#     --yes     aucune question (la clé Claude se règle ensuite dans /etc/printr.env)
# Depuis un autre PC, `scripts/deploy.sh hôte` compile, envoie et lance ce script.
#
# Relancer le script ne perd rien : les données sont vérifiées et sauvegardées avant toute
# modification, la configuration existante est conservée (seules les options nouvelles y sont
# ajoutées), et l'ancien binaire comme les anciens fichiers système sont gardés.
# shellcheck disable=SC1112  # apostrophes typographiques voulues dans les messages
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)

# ---- Affichage ----
if [ -t 1 ]; then
  B=$'\e[1m' D=$'\e[2m' R=$'\e[0m' GREEN=$'\e[32m' YELLOW=$'\e[33m' RED=$'\e[31m' CYAN=$'\e[36m' ORANGE=$'\e[38;5;208m'
else
  B='' D='' R='' GREEN='' YELLOW='' RED='' CYAN='' ORANGE=''
fi
step() { printf '\n%s▸ %s%s\n' "$B$ORANGE" "$1" "$R"; }
ok() { printf '  %s✓%s %s\n' "$GREEN" "$R" "$1"; }
same() { printf '  %s·%s %s\n' "$D" "$R" "$1"; }
warn() { printf '  %s!%s %s\n' "$YELLOW" "$R" "$1"; }
fail() { printf '\n%s✗ %s%s\n' "$RED$B" "$1" "$R" >&2; exit 1; }
note() { printf '    %s%s%s\n' "$D" "$1" "$R"; }
have() { command -v "$1" >/dev/null 2>&1; }

# ---- Options ----
BIN='' OWNER=${SUDO_USER:-} ASK=1
while [ $# -gt 0 ]; do
  case "$1" in
    --binary) BIN=$2; shift ;;
    --user) OWNER=$2; shift ;;
    --yes) ASK=0 ;;
    -h | --help) sed -n '2,14s/^# \{0,1\}//p' "$0"; exit 0 ;;
    *) fail "option inconnue : $1 (voir --help)" ;;
  esac
  shift
done
[ -t 0 ] || ASK=0
if [ -z "$BIN" ]; then
  for candidate in "$HERE/printr" "$HERE/../target/release/printr"; do
    [ -x "$candidate" ] && { BIN=$candidate; break; }
  done
fi

# ---- Vérifications préalables (rien n'est modifié avant la fin de cette partie) ----
[ "$(id -u)" -eq 0 ] || fail "à lancer avec sudo : sudo bash $0"
[ -n "$BIN" ] && [ -x "$BIN" ] || fail "programme printr introuvable (--binary CHEMIN, ou cargo build --release)"
NEW_VERSION=$("$BIN" --version 2>/dev/null) || fail "$BIN ne s'exécute pas ici (compilé pour une autre architecture que $(uname -m) ?)"
[ -d /run/systemd/system ] && have systemctl || fail "systemd est nécessaire pour installer le service"
for tool in useradd groupadd usermod install; do have "$tool" || fail "outil manquant : $tool"; done
for file in printr.service printr.env.example 70-tm-t88v.rules 50-printr.rules; do
  [ -f "$HERE/$file" ] || fail "fichier manquant à côté du script : $file"
done
if [ -z "$OWNER" ] || [ "$OWNER" = root ]; then
  OWNER=
elif ! id "$OWNER" >/dev/null 2>&1; then
  fail "compte inconnu : $OWNER"
fi

DATA=/var/lib/printr
CACHE=/var/cache/printr
ENV=/etc/printr.env
BACKUPS=/var/backups/printr
STAMP=$(date +%Y%m%d-%H%M%S)
OLD_VERSION=$( [ -x /opt/printr/printr ] && /opt/printr/printr --version 2>/dev/null || true )
# Données d'une ancienne version (DynamicUser) : /var/lib/printr -> /var/lib/private/printr.
LIVE_DATA=$(readlink -f "$DATA" 2>/dev/null || echo "$DATA")

printf '\n%s  ┌─────────────────────────────────────┐\n  │   🧾  printr · installation          │\n  └─────────────────────────────────────┘%s\n' "$B" "$R"
note "$(hostname) · $(uname -m) · $( (. /etc/os-release 2>/dev/null && echo "$PRETTY_NAME") || uname -s)"
if [ -n "$OLD_VERSION" ]; then note "mise à jour : $OLD_VERSION → ${NEW_VERSION#printr }"; else note "nouvelle installation : $NEW_VERSION"; fi

# Vérifie les données existantes avant d'y toucher : un fichier illisible arrête tout.
step "Données existantes"
if [ -f "$LIVE_DATA/printr.json" ]; then
  if have python3; then
    summary=$(python3 -I - "$LIVE_DATA/printr.json" <<'EOF'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception as e:
    print(f"ERREUR {e}")
    sys.exit(0)
n = lambda k: len(d.get(k) or [])
print(f"{n('users')} compte(s), {n('presets')} ticket(s) enregistré(s), {n('history')} impression(s) dans l'historique, {n('shopping')} article(s) de courses")
EOF
)
    case "$summary" in
      ERREUR*) fail "$LIVE_DATA/printr.json est illisible (${summary#ERREUR }) : rien n'a été modifié" ;;
      *) ok "$summary" ;;
    esac
  else
    PRINTR_DATA_DIR=$LIVE_DATA "$BIN" -q user list >/dev/null 2>&1 \
      || fail "$LIVE_DATA/printr.json est illisible : rien n'a été modifié"
    ok "fichier de données lisible"
  fi
  photos=$(find "$LIVE_DATA/images" -type f 2>/dev/null | wc -l)
  [ "$photos" -gt 0 ] && ok "$photos photo(s)"
  install -d -m 700 "$BACKUPS"
  tar -czf "$BACKUPS/donnees-$STAMP.tar.gz" -C "$(dirname "$LIVE_DATA")" "$(basename "$LIVE_DATA")"
  ok "sauvegarde : $BACKUPS/donnees-$STAMP.tar.gz"
  # On garde les cinq sauvegardes les plus récentes.
  ls -1t "$BACKUPS"/donnees-*.tar.gz 2>/dev/null | tail -n +6 | xargs -r rm -f
else
  same "aucune donnée : l'appli proposera de créer le premier compte"
fi

# Installe un fichier système ; l'ancienne version, si elle diffère, part dans les sauvegardes.
put() { # put SOURCE DESTINATION MODE [PROPRIÉTAIRE:GROUPE]
  local src=$1 dest=$2 mode=$3 owner=${4:-root:root}
  if [ -f "$dest" ] && cmp -s "$src" "$dest"; then
    chmod "$mode" "$dest"; chown "$owner" "$dest"
    same "$dest inchangé"
    return 1
  fi
  if [ -f "$dest" ]; then
    install -d -m 700 "$BACKUPS"
    cp -p "$dest" "$BACKUPS/$(basename "$dest").$STAMP"
    install -m "$mode" -o "${owner%:*}" -g "${owner#*:}" "$src" "$dest"
    ok "$dest mis à jour (ancienne version dans $BACKUPS)"
  else
    install -D -m "$mode" -o "${owner%:*}" -g "${owner#*:}" "$src" "$dest"
    ok "$dest installé"
  fi
}

# ---- Utilisateur du service ----
step "Utilisateur du service"
getent group printr >/dev/null || groupadd --system printr
getent group lp >/dev/null || groupadd --system lp
if id printr >/dev/null 2>&1; then
  usermod -aG lp printr
  same "utilisateur système printr présent"
else
  useradd --system --gid printr --groups lp --home-dir "$DATA" --no-create-home --shell /usr/sbin/nologin printr
  ok "utilisateur système printr créé"
fi
RELOGIN=0
if [ -n "$OWNER" ]; then
  if id -nG "$OWNER" | tr ' ' '\n' | grep -qx printr; then
    same "$OWNER fait déjà partie du groupe printr"
  else
    RELOGIN=1
    ok "$OWNER ajouté aux groupes printr et lp (administration sans sudo)"
  fi
  usermod -aG printr,lp "$OWNER"
else
  warn "aucun compte administrateur (--user) : la gestion demandera sudo"
fi

# ---- Reprise d'une ancienne version (DynamicUser) ----
systemctl stop printr 2>/dev/null || true
for dir in "$DATA" "$CACHE"; do
  private="$(dirname "$dir")/private/$(basename "$dir")"
  if [ -L "$dir" ] && [ -d "$private" ]; then
    rm "$dir"
    mv "$private" "$dir"
    ok "données de l'ancienne version reprises dans $dir"
  elif [ -d "$dir" ] && [ ! -L "$dir" ] && [ -d "$private" ]; then
    warn "$private existe aussi : conservé tel quel, $dir est utilisé"
  fi
done

# ---- Programme ----
step "Programme"
install -d -m 2775 -o root -g printr /opt/printr
if [ -x /opt/printr/printr ] && cmp -s "$BIN" /opt/printr/printr; then
  same "/opt/printr/printr inchangé ($NEW_VERSION)"
else
  [ -x /opt/printr/printr ] && cp -p /opt/printr/printr /opt/printr/printr.old
  install -m 775 -o root -g printr "$BIN" /opt/printr/printr.new
  mv /opt/printr/printr.new /opt/printr/printr
  ok "/opt/printr/printr ($NEW_VERSION)"
  [ -f /opt/printr/printr.old ] && note "version précédente gardée : /opt/printr/printr.old"
fi
ln -sfn /opt/printr/printr /usr/local/bin/printr

# ---- Dossiers de données ----
step "Dossiers"
for dir in "$DATA" "$CACHE"; do
  install -d -m 2770 -o printr -g printr "$dir"
  chown -R printr:printr "$dir"
  chmod -R g+rwX,o-rwx "$dir"
  find "$dir" -type d -exec chmod g+s {} +
done
ok "$DATA (comptes, tickets, historique, photos)"
ok "$CACHE (contenus du jour)"

# ---- Configuration : on garde l'existante, on ajoute seulement les options nouvelles ----
step "Configuration"
if [ -f "$ENV" ]; then
  added=()
  while IFS= read -r line; do
    key=$(printf '%s' "$line" | sed -n 's/^#\{0,1\}\([A-Z_][A-Z0-9_]*\)=.*/\1/p')
    [ -n "$key" ] || continue
    grep -qE "^#?$key=" "$ENV" && continue
    [ ${#added[@]} -eq 0 ] && printf '\n# Options ajoutées par l’installation du %s :\n' "$(date '+%d/%m/%Y')" >> "$ENV"
    printf '%s\n' "$line" >> "$ENV"
    added+=("$key")
  done < "$HERE/printr.env.example"
  if [ ${#added[@]} -gt 0 ]; then ok "$ENV conservé, options ajoutées : ${added[*]}"; else same "$ENV conservé"; fi
  # Anciennes versions : valeur avec espaces sans guillemets (illisible depuis un shell).
  sed -i 's|^PRINTR_BARNUM=python3 /opt/barnum/main.py$|PRINTR_BARNUM="python3 /opt/barnum/main.py"|' "$ENV"
else
  install -m 660 -o root -g printr "$HERE/printr.env.example" "$ENV"
  ok "$ENV créé"
fi
chown root:printr "$ENV"
chmod 660 "$ENV"
if grep -q '^ANTHROPIC_API_KEY=.\+' "$ENV"; then
  same "clé Claude présente"
elif [ "$ASK" = 1 ]; then
  printf '  %s?%s Clé API Claude (horoscope Claude, mot du jour), ou Entrée pour passer : ' "$CYAN" "$R"
  read -rs key; echo
  if [ -n "$key" ]; then
    grep -q '^ANTHROPIC_API_KEY=' "$ENV" || echo 'ANTHROPIC_API_KEY=' >> "$ENV"
    KEY="$key" awk '/^ANTHROPIC_API_KEY=/ { print "ANTHROPIC_API_KEY=" ENVIRON["KEY"]; next } { print }' "$ENV" > "$ENV.tmp"
    cat "$ENV.tmp" > "$ENV" && rm -f "$ENV.tmp"
    ok "clé Claude enregistrée"
  else
    warn "sans clé : horoscope Claude et mot du jour indisponibles (à ajouter dans $ENV)"
  fi
else
  warn "pas de clé Claude : à ajouter dans $ENV (horoscope Claude et mot du jour)"
fi
value() { sed -n "s/^$1=//p" "$ENV" | tail -1 | tr -d '"'"'"; }

# ---- Imprimante ----
step "Imprimante"
put "$HERE/70-tm-t88v.rules" /etc/udev/rules.d/70-tm-t88v.rules 644 || true
printf 'usblp\n' > /etc/modules-load.d/usblp.conf
modprobe usblp 2>/dev/null || true
have udevadm && { udevadm control --reload; udevadm trigger; }
tcp=$(value PRINTR_TCP)
device=$(value PRINTR_DEVICE)
if [ -n "$tcp" ]; then
  if (exec 3<>"/dev/tcp/${tcp%:*}/${tcp##*:}") 2>/dev/null; then ok "imprimante réseau joignable : $tcp"; else warn "imprimante réseau injoignable pour l'instant : $tcp"; fi
else
  sleep 1
  if [ -e "${device:-/dev/usb/lp0}" ]; then
    ok "imprimante USB détectée : ${device:-/dev/usb/lp0}"
  else
    warn "imprimante non détectée sur ${device:-/dev/usb/lp0} : branche-la et allume-la (réseau : PRINTR_TCP dans $ENV)"
  fi
fi

# ---- Barnum (facultatif) ----
step "Barnum (horoscope hors ligne)"
if ! have python3 || ! have git; then
  warn "python3 et git sont nécessaires : bloc « Horoscope » indisponible"
elif [ -d /opt/barnum/.git ]; then
  if git -C /opt/barnum pull -q 2>/dev/null; then ok "à jour"; else warn "mise à jour impossible, version actuelle conservée"; fi
elif git clone -q https://github.com/XNinety9/Barnum /opt/barnum 2>/dev/null; then
  ok "installé dans /opt/barnum"
else
  warn "téléchargement impossible : bloc « Horoscope » indisponible"
fi

# ---- Service ----
step "Service"
put "$HERE/printr.service" /etc/systemd/system/printr.service 644 || true
if [ -d /etc/polkit-1/rules.d ]; then
  put "$HERE/50-printr.rules" /etc/polkit-1/rules.d/50-printr.rules 644 || true
  note "le groupe printr peut redémarrer le service sans sudo"
else
  warn "polkit absent : systemctl restart printr demandera sudo"
fi
systemctl daemon-reload
systemctl enable -q printr
systemctl restart printr
listen=$(value PRINTR_LISTEN); port=${listen##*:}; port=${port:-8080}
answers() { (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; }
for _ in $(seq 30); do answers && break; sleep 0.5; done
if systemctl is-active -q printr && answers; then
  ok "printr tourne et répond sur le port $port"
else
  journalctl -u printr -n 10 --no-pager -o cat >&2
  [ -f /opt/printr/printr.old ] && note "pour revenir à la version précédente : sudo mv /opt/printr/printr.old /opt/printr/printr && sudo systemctl restart printr"
  fail "le service ne répond pas (journal ci-dessus)"
fi
if have ufw && ufw status 2>/dev/null | grep -q '^Status: active'; then
  ufw status | grep -qE "^$port(/tcp)? " || warn "pare-feu ufw actif : ouvre le port pour le réseau local (sudo ufw allow $port/tcp)"
fi

# ---- Résumé ----
ip=$(ip -4 route get 1.1.1.1 2>/dev/null | sed -n 's/.* src \([0-9.]*\).*/\1/p')
printf '\n%s  Prêt !%s L’appli, depuis un téléphone ou un ordinateur du réseau :\n\n' "$B$GREEN" "$R"
printf '    %shttp://%s.local:%s%s\n' "$B$CYAN" "$(hostname)" "$port" "$R"
[ -n "$ip" ] && printf '    %shttp://%s:%s%s %s(si le nom .local ne marche pas)%s\n' "$CYAN" "$ip" "$port" "$R" "$D" "$R"
if [ -z "$OLD_VERSION" ]; then
  printf '\n  Au premier lancement, l’appli propose de créer ton compte,\n  puis ceux de la famille (menu « Mon compte »).\n'
fi
[ "$RELOGIN" = 1 ] && printf '\n  %sDéconnecte-toi puis reconnecte-toi%s pour administrer printr sans sudo.\n' "$B" "$R"
printf '\n'
note "systemctl restart printr     redémarrer le service"
note "journalctl -u printr -f      suivre les impressions"
note "nano $ENV        configuration (clé Claude, imprimante, port…)"
note "$BACKUPS        sauvegardes des données"
echo
