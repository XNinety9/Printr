#!/usr/bin/env bash
# Builds the Printr image for Raspberry Pi (Pi 3, 4, 5, Zero 2 W, 64-bit) with pi-gen:
# Raspberry Pi OS Lite, plus Printr ready to use (see image/README.md).
#   bash image/build.sh [--binary PATH] [--out DIRECTORY] [--continue]
#     --binary    the printr program for aarch64 (default: built here with cross)
#     --out       where to put the image (default: target/image)
#     --continue  resume an interrupted build: keeps the Raspberry Pi OS already built and only
#                 redoes the printr stage and the export
# Docker is required; on an x86 PC, ARM emulation too (qemu-user-static package).
set -euo pipefail
cd "$(dirname "$0")/.."

# pi-gen version, hence Raspberry Pi OS version, the image is built on.
PI_GEN_TAG=2026-10-06-raspios-trixie-arm64
TARGET=aarch64-unknown-linux-gnu

if [ -t 1 ]; then B=$'\e[1m' R=$'\e[0m' GREEN=$'\e[32m' RED=$'\e[31m' ORANGE=$'\e[38;5;208m'; else B='' R='' GREEN='' RED='' ORANGE=''; fi
step() { printf '\n%s▸ %s%s\n' "$B$ORANGE" "$1" "$R"; }
ok() { printf '  %s✓%s %s\n' "$GREEN" "$R" "$1"; }
fail() { printf '\n%s✗ %s%s\n' "$RED$B" "$1" "$R" >&2; exit 1; }

BIN='' OUT=target/image CONTINUE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --binary) BIN=$2; shift ;;
    --out) OUT=$2; shift ;;
    --continue) CONTINUE=1 ;;
    -h | --help) sed -n '2,9s/^# \{0,1\}//p' "$0"; exit 0 ;;
    *) fail "unknown option: $1 (see --help)" ;;
  esac
  shift
done
command -v docker >/dev/null || fail "Docker is required"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

step "printr $VERSION ($TARGET)"
if [ -z "$BIN" ]; then
  command -v cross >/dev/null || fail "cross is required to build (cargo install cross), or use --binary"
  cross build --release --locked --target "$TARGET" --target-dir target/cross
  BIN=target/cross/$TARGET/release/printr
fi
[ -f "$BIN" ] || fail "program not found: $BIN"
file -b "$BIN" | grep -q aarch64 || fail "$BIN is not built for aarch64"
ok "$BIN"

step "pi-gen $PI_GEN_TAG"
WORK=$OUT/pi-gen
if [ "$CONTINUE" = 1 ]; then
  [ -d "$WORK" ] && docker ps -a --format '{{.Names}}' | grep -qx pigen_work \
    || fail "nothing to resume: run without --continue"
  # Raspberry Pi OS Lite is already in the container: only the printr stage is redone.
  touch "$WORK/stage0/SKIP" "$WORK/stage1/SKIP" "$WORK/stage2/SKIP"
  rm -rf "$WORK/stage-printr" "$WORK/deploy"
else
  rm -rf "$WORK"
  mkdir -p "$OUT"
  git clone -q --depth 1 --branch "$PI_GEN_TAG" https://github.com/RPi-Distro/pi-gen "$WORK" 2>/dev/null
  # Only the final image matters, not the original Lite one.
  touch "$WORK/stage2/SKIP_IMAGES"
fi
cp -r image/stage-printr "$WORK/"
cp image/config "$WORK/config"
FILES=$WORK/stage-printr/00-printr/files
mkdir -p "$FILES"
install -m 755 "$BIN" "$FILES/printr"
cp deploy/printr.service deploy/70-tm-t88v.rules deploy/50-printr.rules "$FILES/"
cp image/printr.env image/image.conf image/51-printr-network.rules image/printr-portal.conf "$FILES/"
git clone -q --depth 1 https://github.com/XNinety9/Barnum "$FILES/barnum"
ok "printr stage ready"

step "Building (from 20 minutes to over an hour)"
# A container left behind by an interrupted build would block the next one.
[ "$CONTINUE" = 1 ] || docker rm -v pigen_work >/dev/null 2>&1 || true
# The host's /dev: loop devices created during the export must show up in the container.
export PIGEN_DOCKER_OPTS="${PIGEN_DOCKER_OPTS:-} --volume /dev:/dev"
(cd "$WORK" && CONTINUE=$CONTINUE ./build-docker.sh -c config)
img=$(ls -t "$WORK"/deploy/*.img.xz | head -1)
dest="$OUT/printr-$VERSION-raspberrypi-arm64.img.xz"
mv "$img" "$dest"
(cd "$OUT" && sha256sum "$(basename "$dest")" > "$(basename "$dest").sha256")
ok "$dest ($(du -h "$dest" | cut -f1))"
printf '\n  Write it to an SD card with Raspberry Pi Imager ("Use custom").\n\n'
