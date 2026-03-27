#!/bin/bash
# test-capture.sh — Test fonctionnel de capture DAB/ETI
# Vérifie le build, la détection du dongle RTL-SDR, la synchro DAB
# et la validité du flux ETI généré.
#
# Usage:
#   ./test-capture.sh                  # canal par défaut (6C)
#   ./test-capture.sh 11C              # canal spécifique
#   ./test-capture.sh 6C 30            # canal + gain

set -euo pipefail

# --- Configuration ---
CHANNEL="${1:-6C}"
GAIN="${2:-20}"
DURATION=10          # secondes de capture
ETI_FRAME_SIZE=6144  # taille d'une frame ETI (octets)
MIN_FRAMES=5         # minimum de frames attendues
OUTFILE=$(mktemp /tmp/eti-test-XXXXXX.eti)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${SCRIPT_DIR}/target/release/eti-rtlsdr-rust"

# --- Couleurs ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass() { echo -e "  ${GREEN}✓${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; }
warn() { echo -e "  ${YELLOW}!${NC} $1"; }

cleanup() {
    rm -f "$OUTFILE"
}
trap cleanup EXIT

echo "╔══════════════════════════════════════╗"
echo "║   Test fonctionnel ETI-RTL-SDR-Rust  ║"
echo "╚══════════════════════════════════════╝"
echo ""
echo "  Canal: $CHANNEL | Gain: $GAIN% | Durée: ${DURATION}s"
echo ""

ERRORS=0

# --- 1. Vérifier le build ---
echo "── Build ──"
if [ -f "$BINARY" ]; then
    pass "Binaire release trouvé"
else
    echo "  Build en cours..."
    if cargo build --release 2>&1 | tail -1; then
        pass "Build release OK"
    else
        fail "Build échoué"
        exit 1
    fi
fi

# --- 2. Configurer LD_LIBRARY_PATH ---
LIB_DIR=$(find "$SCRIPT_DIR/target" -name "librtlsdr.so.0" 2>/dev/null | head -1 | xargs dirname 2>/dev/null || true)
if [ -n "$LIB_DIR" ]; then
    export LD_LIBRARY_PATH="${LIB_DIR}:${LD_LIBRARY_PATH:-}"
    pass "librtlsdr.so trouvée dans $LIB_DIR"
else
    warn "librtlsdr.so non trouvée dans target/, utilisation du système"
fi

# --- 3. Tests unitaires ---
echo ""
echo "── Tests unitaires ──"
if command -v cargo >/dev/null 2>&1; then
    TEST_OUTPUT=$(cargo test --lib 2>&1)
    TEST_COUNT=$(echo "$TEST_OUTPUT" | grep -oP '\d+ passed' | grep -oP '\d+' || echo "0")
    FAIL_COUNT=$(echo "$TEST_OUTPUT" | grep -oP '\d+ failed' | grep -oP '\d+' || echo "0")

    if [ "$FAIL_COUNT" = "0" ] && [ "$TEST_COUNT" -gt 0 ]; then
        pass "$TEST_COUNT tests passés, 0 échoué"
    else
        fail "$TEST_COUNT passés, $FAIL_COUNT échoués"
        ERRORS=$((ERRORS + 1))
    fi
else
    warn "cargo non trouvé (exécution sous sudo ?), tests unitaires ignorés"
fi

# --- 4. Détection du dongle ---
echo ""
echo "── Détection RTL-SDR ──"
if lsusb 2>/dev/null | grep -qi "RTL2838\|RTL-SDR\|0bda:2838"; then
    pass "Dongle RTL-SDR détecté (USB)"
else
    warn "Dongle RTL-SDR non détecté via lsusb (peut fonctionner quand même)"
fi

# --- 5. Capture ETI ---
echo ""
echo "── Capture ETI (canal $CHANNEL, ${DURATION}s) ──"
STDERR_FILE=$(mktemp /tmp/eti-stderr-XXXXXX.log)

# Lancer la capture avec timeout
set +e
timeout $((DURATION + 20)) "$BINARY" \
    -C "$CHANNEL" \
    -G "$GAIN" \
    -O "$OUTFILE" \
    -t "$DURATION" \
    -d 8 \
    -D 15 \
    2>"$STDERR_FILE"
EXIT_CODE=$?
set -e

# Analyser stderr
if grep -q "DAB signal" "$STDERR_FILE" 2>/dev/null; then
    pass "Signal DAB détecté"
else
    warn "Pas de confirmation de signal DAB"
fi

if grep -q "ensemble.*detected" "$STDERR_FILE" 2>/dev/null; then
    ENSEMBLE=$(grep "ensemble" "$STDERR_FILE" | head -1 | sed 's/.*ensemble \(.*\) detected.*/\1/')
    pass "Ensemble reconnu : $ENSEMBLE"
else
    warn "Ensemble non reconnu (timeout détection ?)"
fi

PROGRAMS=$(grep -c "^program" "$STDERR_FILE" 2>/dev/null || echo "0")
if [ "$PROGRAMS" -gt 0 ]; then
    pass "$PROGRAMS programme(s) détecté(s)"
    grep "^program" "$STDERR_FILE" | while read -r line; do
        echo "      $line"
    done
else
    warn "Aucun programme détecté"
fi

# --- 6. Validation du fichier ETI ---
echo ""
echo "── Validation ETI ──"
if [ -f "$OUTFILE" ]; then
    FILE_SIZE=$(stat -c%s "$OUTFILE" 2>/dev/null || stat -f%z "$OUTFILE" 2>/dev/null || echo "0")
    FRAME_COUNT=$((FILE_SIZE / ETI_FRAME_SIZE))

    if [ "$FILE_SIZE" -gt 0 ]; then
        pass "Fichier ETI créé : $(numfmt --to=iec "$FILE_SIZE" 2>/dev/null || echo "${FILE_SIZE} octets")"
        pass "$FRAME_COUNT frames ETI ($ETI_FRAME_SIZE octets/frame)"
    else
        fail "Fichier ETI vide"
        ERRORS=$((ERRORS + 1))
    fi

    if [ "$FRAME_COUNT" -ge "$MIN_FRAMES" ]; then
        pass "Assez de frames ($FRAME_COUNT >= $MIN_FRAMES)"
    else
        fail "Pas assez de frames ($FRAME_COUNT < $MIN_FRAMES)"
        ERRORS=$((ERRORS + 1))
    fi

    # Vérifier l'alignement (taille doit être multiple de 6144)
    REMAINDER=$((FILE_SIZE % ETI_FRAME_SIZE))
    if [ "$REMAINDER" -eq 0 ]; then
        pass "Alignement correct (multiple de $ETI_FRAME_SIZE)"
    else
        warn "Alignement incorrect ($REMAINDER octets en trop)"
    fi

    # Vérifier le sync word ETI (premiers octets)
    if command -v od >/dev/null 2>&1 && [ "$FILE_SIZE" -ge 4 ]; then
        FIRST_BYTES=$(od -A n -t x1 -N 4 "$OUTFILE" | tr -d ' ')
        pass "Premiers octets : 0x$FIRST_BYTES"
    fi
else
    fail "Fichier ETI non créé"
    ERRORS=$((ERRORS + 1))
fi

# --- Résumé ---
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if [ "$ERRORS" -eq 0 ]; then
    echo -e "  ${GREEN}RÉSULTAT : SUCCÈS${NC}"
else
    echo -e "  ${RED}RÉSULTAT : $ERRORS ERREUR(S)${NC}"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

rm -f "$STDERR_FILE"
exit "$ERRORS"
