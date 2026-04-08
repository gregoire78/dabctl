#!/bin/bash
# Capture directe RTL-SDR → PCM via iq2pcm (pipeline en mémoire, sans ETI intermédiaire).
# Usage : ./live-capture-iq2pcm.sh [CHANNEL] [SID] [GAIN]
#   CHANNEL : canal DAB Band III (défaut : 6C)
#   SID     : identifiant de service en hex (défaut : 0xF2F8  NRJ)
#   GAIN    : gain en % 0-100 (défaut : 30)
set -e

CHANNEL="${1:-6C}"
SID="${2:-0xF2F8}"
GAIN="${3:-30}"

mkdir -p test-local
cd "$(dirname "$0")/test-local" || { echo "[ERREUR] Impossible de changer de répertoire"; exit 1; }

# Construire en release et lancer les tests avant la capture
echo "[build] Compilation en release..."
pushd .. > /dev/null
cargo test --lib || { echo "[ERREUR] Tests unitaires échoués"; exit 1; }
cargo build --release || { echo "[ERREUR] Build release échoué"; exit 1; }
popd > /dev/null

# Nettoyer les anciens fichiers
rm -f output.wav pad_metadata.json iq2pcm.log
rm -f slides/*.jpg 2>/dev/null || true
mkdir -p slides

echo "[capture] Canal=${CHANNEL}  SID=${SID}  Gain=${GAIN}%"
echo "[capture] Ctrl-C pour arrêter"

# Pipeline direct : RTL-SDR → décodage DAB/DAB+ → PCM
sudo ../target/release/dabctl iq2pcm \
  -C "$CHANNEL" \
  -s "$SID" \
  -G "$GAIN" \
  --slide-dir ./slides \
  --slide-base64 \
  -m pad_metadata.json \
  2>iq2pcm.log \
  | sox -t raw -r 48000 -b 16 -c 2 -e signed-integer -L - output.wav

echo -e "\n--- Résultats ---"
ls -lh output.wav pad_metadata.json iq2pcm.log
ls -lh slides/ 2>/dev/null || true
