#!/bin/sh
# Garde des mises à jour automatiques : lancé par systemd avant chaque démarrage de printr
# (ExecStartPre). Volontairement en shell, indépendant du programme : il doit marcher même si
# la nouvelle version est cassée.
#
# Après une mise à jour, printr écrit `update-pending` (ancienne et nouvelle version) dans son
# dossier de données, puis redémarre. Au premier démarrage, ce script note la tentative. Si la
# nouvelle version confirme qu'elle répond, elle efface le fichier. Sinon (plantage, port muet),
# elle s'arrête ; au démarrage suivant, ce script remet l'ancienne version en place et inscrit
# la nouvelle dans `update-refused`, pour ne plus jamais la réessayer.
set -eu
STATE=${STATE_DIRECTORY:-/var/lib/printr}
BIN=${PRINTR_BIN:-/opt/printr/printr}
PENDING="$STATE/update-pending"

[ -f "$PENDING" ] || exit 0
STARTS=$(cat "$PENDING.starts" 2>/dev/null || echo 0)

if [ "$STARTS" -ge 1 ] && [ -f "$BIN.old" ]; then
  FROM=$(sed -n 's/^from=//p' "$PENDING")
  TO=$(sed -n 's/^to=//p' "$PENDING")
  mv -f "$BIN.old" "$BIN"
  echo "$TO" >> "$STATE/update-refused"
  rm -f "$PENDING" "$PENDING.starts"
  echo "printr $TO n'a pas confirmé son démarrage : retour à $FROM, et $TO ne sera plus installée" >&2
else
  echo $((STARTS + 1)) > "$PENDING.starts"
fi
