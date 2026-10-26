#!/usr/bin/env bash
# Installe ou met à jour Printr en une commande, sur un Raspberry Pi déjà en service (ou tout Linux
# avec systemd) :
#   curl -fsSL https://xninety9.github.io/Printr/install.sh | bash
# Le script télécharge l'archive de la dernière version pour cette machine, vérifie la signature de
# la release (clés compilées ci-dessous, avec openssl) puis l'empreinte de l'archive, et lance son
# install.sh (le mot de passe sudo est demandé une fois). La suite se fait dans le navigateur.
# Options, après « bash -s -- » :
#   --version vX.Y.Z   une version précise plutôt que la dernière
#   --download-only    télécharger et vérifier, sans installer (le dossier est affiché)
# shellcheck disable=SC1112  # apostrophes typographiques voulues dans les messages
set -euo pipefail

# Clés publiques des releases (les mêmes que RELEASE_KEYS dans src/update.rs) : la principale,
# puis celle de secours.
KEYS=(
  RWRIN2zEgDpje3QxNPTg22HNP4s60jR0Dc6N4zw0oLyuUysCbJGEs7/n
  RWQHgDOUKqaMsuFufsLr8suDK3VHNHxYlrk5L/tehgVsLWsVzzzLZCp/
)
REPO=XNinety9/Printr
# Pour les essais seulement : un autre dépôt de releases, et ses clés.
BASE=${PRINTR_RELEASE_BASE:-https://github.com/$REPO/releases}
API=${PRINTR_RELEASE_API:-https://api.github.com/repos/$REPO/releases/latest}
[ -n "${PRINTR_RELEASE_KEY:-}" ] && KEYS=("$PRINTR_RELEASE_KEY")

if [ -t 1 ]; then
  B=$'\e[1m' D=$'\e[2m' R=$'\e[0m' GREEN=$'\e[32m' RED=$'\e[31m' ORANGE=$'\e[38;5;208m'
else
  B='' D='' R='' GREEN='' RED='' ORANGE=''
fi
step() { printf '\n%s▸ %s%s\n' "$B$ORANGE" "$1" "$R"; }
ok() { printf '  %s✓%s %s\n' "$GREEN" "$R" "$1"; }
note() { printf '    %s%s%s\n' "$D" "$1" "$R"; }
fail() { printf '\n%s✗ %s%s\n' "$RED$B" "$1" "$R" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# verify_minisig CLÉ FICHIER SIGNATURE : signature minisign (Ed25519 sur l'empreinte BLAKE2b-512
# du fichier), vérifiée avec openssl, sans avoir à installer minisign.
verify_minisig() {
  local pub=$1 file=$2 sig=$3 tmp result
  tmp=$(mktemp -d)
  if printf '%s' "$pub" | base64 -d >"$tmp/pub" 2>/dev/null \
    && sed -n 2p "$sig" | base64 -d >"$tmp/sig" 2>/dev/null \
    && [ "$(head -c 2 "$tmp/pub")" = Ed ] && [ "$(head -c 2 "$tmp/sig")" = ED ] \
    && [ "$(dd if="$tmp/pub" bs=1 skip=2 count=8 2>/dev/null | od -An -tx1)" = "$(dd if="$tmp/sig" bs=1 skip=2 count=8 2>/dev/null | od -An -tx1)" ]; then
    # Clé publique Ed25519 brute → PEM (en-tête DER fixe), signature : les 64 derniers octets.
    { printf '\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00'; tail -c 32 "$tmp/pub"; } >"$tmp/der"
    tail -c 64 "$tmp/sig" >"$tmp/sig64"
    openssl pkey -pubin -inform DER -in "$tmp/der" -out "$tmp/pem" 2>/dev/null \
      && openssl dgst -blake2b512 -binary "$file" >"$tmp/digest" \
      && openssl pkeyutl -verify -pubin -inkey "$tmp/pem" -rawin -in "$tmp/digest" -sigfile "$tmp/sig64" >/dev/null 2>&1
    result=$?
  else
    result=1
  fi
  rm -rf "$tmp"
  return "$result"
}

main() {
  local version='' only=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --version) version=$2; shift ;;
      --download-only) only=1 ;;
      -h | --help) sed -n '2,11s/^# \{0,1\}//p' "$0"; exit 0 ;;
      *) fail "option inconnue : $1" ;;
    esac
    shift
  done

  printf '\n%s  🧾  Printr · installation%s\n' "$B" "$R"
  for tool in curl tar sha256sum openssl base64 od; do have "$tool" || fail "outil manquant : $tool"; done
  [ "$only" = 1 ] || [ "$(id -u)" -ne 0 ] || fail "à lancer sans sudo, depuis ton compte : le mot de passe sera demandé ensuite"

  local target
  case "$(uname -m)" in
    aarch64 | arm64) target=aarch64-unknown-linux-gnu ;;
    armv7l) target=armv7-unknown-linux-gnueabihf ;;
    armv6l) target=arm-unknown-linux-gnueabihf ;;
    x86_64) target=x86_64-unknown-linux-gnu ;;
    *) fail "machine non prise en charge : $(uname -m)" ;;
  esac

  step "Version"
  if [ -z "$version" ]; then
    version=$(curl -fsSL "$API" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1) \
      || fail "impossible de joindre GitHub"
  fi
  [[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "version illisible : ${version:-aucune}"
  ok "Printr $version pour $target"

  step "Téléchargement et vérification"
  local dir archive
  dir=$(mktemp -d)
  archive="printr-$version-$target.tar.gz"
  for file in SHA256SUMS SHA256SUMS.minisig "$archive"; do
    curl -fsSL -o "$dir/$file" "$BASE/download/$version/$file" || fail "$file introuvable dans la release $version"
  done
  local signed=0
  for key in "${KEYS[@]}"; do
    verify_minisig "$key" "$dir/SHA256SUMS" "$dir/SHA256SUMS.minisig" && signed=1
  done
  [ "$signed" = 1 ] || fail "signature de la release invalide : rien n'est installé"
  ok "release signée par l'auteur de Printr"
  (cd "$dir" && grep -E "  \*?$archive\$" SHA256SUMS | sha256sum --quiet -c -) \
    || fail "empreinte de l'archive incorrecte : fichier corrompu ou modifié, rien n'est installé"
  ok "archive intacte ($archive)"
  tar -C "$dir" -xzf "$dir/$archive"

  if [ "$only" = 1 ]; then
    ok "téléchargée et vérifiée, sans installation"
    note "$dir/${archive%.tar.gz}"
    return
  fi
  step "Installation"
  note "le mot de passe sudo est demandé une seule fois"
  sudo bash "$dir/${archive%.tar.gz}/install.sh" </dev/null
  rm -rf "$dir"
}

# Tout est dans main : une copie interrompue en cours de téléchargement ne lance rien.
main "$@"
