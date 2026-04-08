#!/bin/bash
# Capture directe RTL-SDR → PCM (pipeline en mémoire).
# Usage : ./live-capture-iq2pcm.sh [CHANNEL] [SID] [GAIN]
#   CHANNEL : canal DAB Band III (défaut : 6C)
#   SID     : identifiant de service en hex (défaut : 0xF2F8  NRJ)
#   GAIN    : gain en % 0-100 (si omis → auto-gain)
set -e

CHANNEL="${1:-6C}"
SID="${2:-0xF2F8}"
GAIN="${3:-75}"

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

GAIN_DISPLAY="${GAIN:-auto}"
echo "[capture] Canal=${CHANNEL}  SID=${SID}  Gain=${GAIN_DISPLAY}"
echo "[capture] Ctrl-C pour arrêter"

# Construire les arguments de gain
GAIN_ARGS=()
if [ -n "$GAIN" ]; then
  GAIN_ARGS=(-G "$GAIN")
fi

# Pipeline direct : RTL-SDR → décodage DAB/DAB+ → PCM
# sudo closes inherited fd >= 3; open fd 3 inside the sudo shell.
sudo sh -c 'exec 3>pad_metadata.json; exec "$@"' _ \
  ../target/release/dabctl \
  -C "$CHANNEL" \
  -s "$SID" \
  "${GAIN_ARGS[@]}" \
  --slide-dir ./slides \
  --slide-base64 \
  2>iq2pcm.log \
  | ffmpeg -y -f s16le -ar 48000 -ac 2 -i pipe:0 output.wav

echo -e "\n--- Résultats ---"
ls -lh output.wav pad_metadata.json iq2pcm.log
ls -lh slides/ 2>/dev/null || true
