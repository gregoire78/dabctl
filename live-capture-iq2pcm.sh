#!/bin/bash
# Décodage ETI → PCM via dabctl dablin (pipeline en mémoire).
#
# Usage :
#   ./eti-capture-dablin.sh [ETI_FILE] [SID]
#
#   ETI_FILE : fichier ETI (défaut : multiplex.eti)
#   SID      : identifiant de service en hex (défaut : 0xF2F8)

set -e

ETI_FILE="${1:-multiplex.eti}"
SID="${2:-0xF2F8}"

RUN_TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
LOG_FILE="dablin-${RUN_TIMESTAMP}.log"

mkdir -p test-local
cd "$(dirname "$0")/test-local" || {
  echo "[ERREUR] Impossible de changer de répertoire"
  exit 1
}

# Build + tests avant lancement
echo "[build] Tests unitaires…"
pushd .. > /dev/null
rtk cargo test || {
  echo "[ERREUR] Tests échoués"
  exit 1
}

echo "[build] Build release…"
rtk cargo build --release --features fdk-aac || {
  echo "[ERREUR] Build release échoué"
  exit 1
}
popd > /dev/null

# Nettoyage
rm -f output.wav pad_metadata.json
rm -rf slides
mkdir -p slides

echo "[dablin] ETI=${ETI_FILE}"
echo "[dablin] SID=${SID}"
echo "[dablin] Log=${LOG_FILE}"
echo "[dablin] Ctrl-C pour arrêter"

# Pipeline ETI → DAB/DAB+ → PCM
# FD 3 réservé aux métadonnées PAD
sudo RUST_LOG="info,dablin=${RUST_LOG:-info}" sh -c '
  exec 3>pad_metadata.json
  exec "$@"
' _ \
  ../target/release/dabctl dablin \
    -i "$ETI_FILE" \
    -s "$SID" \
    --slide-dir ./slides \
    --slide-base64 \
    --aac-decoder fdk \
  2>"$LOG_FILE" \
| ffmpeg -y \
    -f s16le -ar 48000 -ac 2 \
    -i pipe:0 \
    output.wav

echo
echo "--- Résultats ---"
ls -lh output.wav pad_metadata.json "$LOG_FILE"
ls -lh slides/ 2>/dev/null || true