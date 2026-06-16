#!/bin/bash
# Decodage ETI -> LATM/LOAS via dabctl dablin (pipeline en memoire).
#
# Usage :
#   ./live-capture-one-service-latm.sh [ETI_FILE] [SID]
#
#   ETI_FILE : fichier ETI (defaut : multiplex.eti)
#   SID      : identifiant de service en hex (defaut : 0xF201)

set -e

ETI_FILE="${1:-multiplex.eti}"
SID="${2:-0xF201}"

RUN_TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
LOG_FILE="dablin-latm-${RUN_TIMESTAMP}.log"

mkdir -p test-local
cd "$(dirname "$0")/test-local" || {
  echo "[ERREUR] Impossible de changer de repertoire"
  exit 1
}

# Build + tests avant lancement
echo "[build] Tests unitaires..."
pushd .. > /dev/null
rtk cargo test || {
  echo "[ERREUR] Tests echoues"
  exit 1
}

echo "[build] Build release..."
rtk cargo build --release --features fdk-aac || {
  echo "[ERREUR] Build release echoue"
  exit 1
}
popd > /dev/null

# Nettoyage
rm -f output.latm output.wav pad_metadata.jsonl "$LOG_FILE"
rm -rf slides
mkdir -p slides

echo "[dablin-latm] ETI=${ETI_FILE}"
echo "[dablin-latm] SID=${SID}"
echo "[dablin-latm] Flux LATM=output.latm"
echo "[dablin-latm] Decode WAV=output.wav"
echo "[dablin-latm] Metadonnees=pad_metadata.jsonl"
echo "[dablin-latm] Log=${LOG_FILE}"
echo "[dablin-latm] Ctrl-C pour arreter"

# Pipeline ETI -> DAB/DAB+ -> LATM/LOAS
# FD 3 reserve aux metadonnees PAD
sudo sh -c '
  exec 3>pad_metadata.jsonl
  RUST_LOG=info exec "$@"
' _ \
  ../target/release/dabctl dablin one-service-out \
    -i "$ETI_FILE" \
    -s "$SID" \
    --audio-out latm \
    --slide-dir ./slides \
    --slide-base64 \
    --dedup-pad \
    --datetime-format %H:%M:%S.%3f \
  2>"$LOG_FILE" \
| tee output.latm \
| ffmpeg -y \
    -f loas \
    -i pipe:0 \
    output.wav

echo
echo "--- Resultats ---"
ls -lh output.latm output.wav pad_metadata.jsonl "$LOG_FILE"
ls -lh slides/ 2>/dev/null || true
