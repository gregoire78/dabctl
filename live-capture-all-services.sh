#!/bin/bash
# Export ETI -> arborescence par service via dabctl dablin.
#
# Usage :
#   ./live-capture-all-services.sh [ETI_FILE]
#
#   ETI_FILE : fichier ETI (defaut : multiplex.eti)

set -e

ETI_FILE="${1:-multiplex.eti}"
OUT_DIR="all-services"

RUN_TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
LOG_FILE="dablin-all-services-${RUN_TIMESTAMP}.log"

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
rm -f "$LOG_FILE"
rm -rf "$OUT_DIR"

echo "[dablin] ETI=${ETI_FILE}"
echo "[dablin] Sortie=./${OUT_DIR}"
echo "[dablin] Log=${LOG_FILE}"
echo "[dablin] Ctrl-C pour arreter"

RUST_LOG=info ../target/release/dabctl dablin all-services-out \
  -i "$ETI_FILE" \
  --out "./${OUT_DIR}" \
  --slide-base64 \
  --aac-decoder fdk \
  --aac-gap silence \
  --dedup-pad \
  --datetime-format \
  2>"$LOG_FILE"

echo
echo "--- Resultats ---"
ls -lh "$LOG_FILE"
find "$OUT_DIR" -maxdepth 2 \( -type f -o -type d \) | sort