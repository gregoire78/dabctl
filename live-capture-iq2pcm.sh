#!/bin/bash
# Capture directe RTL-SDR → PCM (pipeline en mémoire).
# Usage : ./live-capture-iq2pcm.sh [CHANNEL] [SID] [GAIN] [TRACE_OFDM]
#   CHANNEL : canal DAB Band III (défaut : 6C)
#   SID     : identifiant de service en hex (défaut : 0xF2F8  NRJ)
#   GAIN    : gain en % 0-100 (si omis → auto-gain)
#   TRACE_OFDM : 1/true/on pour activer --trace-ofdm, sinon désactivé par défaut
set -e

#CHANNEL="${1:-8C}"
#SID="${2:-0xF201}"
CHANNEL="${1:-6C}"
SID="${2:-0xF2F8}"
GAIN="${3:-}"
TRACE_OFDM="${4:-0}"
RUN_TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
LOG_FILE="iq2pcm-${RUN_TIMESTAMP}.log"

mkdir -p test-local
cd "$(dirname "$0")/test-local" || { echo "[ERREUR] Impossible de changer de répertoire"; exit 1; }

# Construire en release et lancer les tests avant la capture
echo "[build] Compilation en release..."
pushd .. > /dev/null
rtk cargo test --lib || { echo "[ERREUR] Tests unitaires échoués"; exit 1; }
rtk cargo build --release --features fdk-aac || { echo "[ERREUR] Build release échoué"; exit 1; }
popd > /dev/null

# Nettoyer les anciens fichiers
rm -f output.wav pad_metadata.json
rm -f slides/* 2>/dev/null || true
mkdir -p slides

GAIN_DISPLAY="${GAIN:-auto}"
echo "[capture] Canal=${CHANNEL}  SID=${SID}  Gain=${GAIN_DISPLAY}"
echo "[capture] Log=${LOG_FILE}"
echo "[capture] Ctrl-C pour arrêter"

# Construire les arguments de gain
GAIN_ARGS=()
if [ -n "$GAIN" ]; then
  GAIN_ARGS=(-G "$GAIN")
fi

# Activer les traces OFDM détaillées à la demande
TRACE_ARGS=()
case "${TRACE_OFDM,,}" in
  1|true|on|yes)
    TRACE_ARGS=(--trace-ofdm)
    ;;
esac

# Pipeline direct : RTL-SDR → décodage DAB/DAB+ → PCM
# sudo closes inherited fd >= 3; open fd 3 inside the sudo shell.
# RUST_LOG is passed explicitly because sudo strips the environment.
sudo RUST_LOG="info,dabctl=${RUST_LOG:-trace}" sh -c 'exec 3>pad_metadata.json; exec "$@"' _ \
  ../target/release/dabctl \
  -C "$CHANNEL" \
  -s "$SID" \
  "${GAIN_ARGS[@]}" \
  "${TRACE_ARGS[@]}" \
  --slide-dir ./slides \
  --slide-base64 \
  --aac-decoder fdk-aac \
  2>"$LOG_FILE" \
  | ffmpeg -y -f s16le -ar 48000 -ac 2 -i pipe:0 output.wav

echo -e "\n--- Résultats ---"
ls -lh output.wav pad_metadata.json "$LOG_FILE"
ls -lh slides/ 2>/dev/null || true
