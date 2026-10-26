#!/usr/bin/env bash
# Signe une release de printr sur ce PC, pour que les Pi en mise à jour automatique l'acceptent.
# La clé privée ne quitte jamais ce PC : un compte GitHub piraté ne suffit pas à pousser une mise
# à jour sur les machines.
#   scripts/signer-release.sh vX.Y.Z [--key FICHIER]   (défaut : ~/.config/printr-release/printr.key)
#   (--key avec la clé de secours, si la principale est perdue : voir CONTRIBUTING)
# 1. télécharge SHA256SUMS et tous les fichiers qu'elle liste ;
# 2. vérifie leurs empreintes : ce qui est signé est bien ce qui est publié ;
# 3. signe SHA256SUMS (le mot de passe de la clé est demandé), vérifie la signature avec les clés
#    publiques compilées dans printr (RELEASE_KEYS, src/update.rs), puis l'ajoute à la release
#    (SHA256SUMS.minisig).
# Les clés se créent une fois (voir CONTRIBUTING) : la principale dans ~/.config/printr-release,
# celle de secours rangée hors ligne ; leurs clés publiques vont dans RELEASE_KEYS.
# shellcheck disable=SC1112  # apostrophes typographiques voulues dans les messages
set -euo pipefail
cd "$(dirname "$0")/.."

REPO=XNinety9/Printr
if [ -t 1 ]; then B=$'\e[1m' R=$'\e[0m' GREEN=$'\e[32m' RED=$'\e[31m' ORANGE=$'\e[38;5;208m'; else B='' R='' GREEN='' RED='' ORANGE=''; fi
step() { printf '\n%s▸ %s%s\n' "$B$ORANGE" "$1" "$R"; }
ok() { printf '  %s✓%s %s\n' "$GREEN" "$R" "$1"; }
fail() { printf '\n%s✗ %s%s\n' "$RED$B" "$1" "$R" >&2; exit 1; }

TAG='' KEY=$HOME/.config/printr-release/printr.key
while [ $# -gt 0 ]; do
  case "$1" in
    --key) KEY=$2; shift ;;
    -h | --help) sed -n '2,13s/^# \{0,1\}//p' "$0"; exit 0 ;;
    v*) TAG=$1 ;;
    *) fail "option inconnue : $1 (voir --help)" ;;
  esac
  shift
done
[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "usage : scripts/signer-release.sh vX.Y.Z"
command -v gh >/dev/null || fail "gh (GitHub CLI) est nécessaire"
[ -f "$KEY" ] || fail "clé privée introuvable : $KEY (voir --help pour la créer)"
mapfile -t PUBS < <(sed -n '/^const RELEASE_KEYS/,/];/p' src/update.rs | grep -o '"[A-Za-z0-9+/=]\{40,\}"' | tr -d '"')
[ "${#PUBS[@]}" -gt 0 ] || fail "RELEASE_KEYS est vide dans src/update.rs : les Pi n'accepteraient aucune signature"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cargo build -q --example signer

step "Release $TAG"
gh release download "$TAG" --repo "$REPO" --dir "$WORK" --pattern SHA256SUMS
mapfile -t files < <(awk '{ sub(/^\*/, "", $2); print $2 }' "$WORK/SHA256SUMS")
[ "${#files[@]}" -gt 0 ] || fail "SHA256SUMS est vide"
for f in "${files[@]}"; do gh release download "$TAG" --repo "$REPO" --dir "$WORK" --pattern "$f"; done
(cd "$WORK" && sha256sum --quiet -c SHA256SUMS) || fail "une empreinte ne correspond pas : rien n'est signé"
ok "${#files[@]} fichiers, empreintes vérifiées"

step "Signature"
cargo run -q --example signer -- sign "$KEY" "$WORK/SHA256SUMS" >/dev/null
valid=0
for pub in "${PUBS[@]}"; do
  cargo run -q --example signer -- verify "$pub" "$WORK/SHA256SUMS" >/dev/null 2>&1 && valid=1
done
[ "$valid" = 1 ] || fail "la signature ne correspond à aucune clé publique de printr (RELEASE_KEYS) : rien n'est envoyé"
ok "SHA256SUMS signée, et vérifiée avec les clés compilées dans printr"
gh release upload "$TAG" "$WORK/SHA256SUMS.minisig" --repo "$REPO" --clobber
ok "SHA256SUMS.minisig ajoutée à la release : les Pi en mise à jour automatique l'installeront"
echo
