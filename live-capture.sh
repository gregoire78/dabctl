#!/bin/bash
set -e
mkdir -p test-local
cd "$(dirname "$0")/test-local" || { echo "[ERREUR] Impossible de changer de répertoire"; exit 1; }

# Construire en release et lancer les tests avant la capture
echo "[build] Compilation en release..."
pushd .. > /dev/null
cargo test --lib || { echo "[ERREUR] Tests unitaires échoués"; exit 1; }
cargo build --release || { echo "[ERREUR] Build release échoué"; exit 1; }
popd > /dev/null

# Définir LD_LIBRARY_PATH pour librtlsdr
export LD_LIBRARY_PATH=$(find ../target -name "librtlsdr.so.0" 2>/dev/null | head -1 | xargs dirname):$LD_LIBRARY_PATH

# Nettoyer les anciens fichiers
rm -f output.wav pad_metadata.json eti2pcm.log iq2eti.log
rm -f slides/*.jpg 2>/dev/null || true
mkdir -p slides

# Lancer la capture live 120s sur 6C NRJ
sudo ../target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 -t 300 2>iq2eti.log \
  | ../target/release/eti-rtlsdr-rust eti2pcm -s 0xF211 -p --slide-dir ./slides --slide-base64 3>pad_metadata.json 2>eti2pcm.log \
  | sox -t raw -r 48000 -b 16 -c 2 -e signed-integer -L - output.wav

echo "\n--- Résultats ---"
ls -lh output.wav pad_metadata.json eti2pcm.log iq2eti.log
ls -lh slides/
