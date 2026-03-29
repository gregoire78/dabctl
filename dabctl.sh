#!/bin/bash

# Script helper pour exécuter dabctl avec LD_LIBRARY_PATH configuré automatiquement

set -e

# Déterminer le répertoire du script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Trouver le binaire release
BINARY="${SCRIPT_DIR}/target/release/dabctl"

# Vérifier que le binaire existe
if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY"
    echo "Please run: cargo build --release"
    exit 1
fi

# Trouver le répertoire contenant librtlsdr.so
LIB_DIR=$(find "$SCRIPT_DIR/target" -name "librtlsdr.so.0" 2>/dev/null | head -1 | xargs dirname)

if [ -z "$LIB_DIR" ]; then
    echo "Warning: librtlsdr.so not found in target directory"
    echo "Make sure to build the project first: cargo build --release"
    LIB_DIR=""
fi

# Exécuter le programme avec LD_LIBRARY_PATH configuré
if [ -z "$LIB_DIR" ]; then
    exec "$BINARY" "$@"
else
    export LD_LIBRARY_PATH="${LIB_DIR}:${LD_LIBRARY_PATH}"
    exec "$BINARY" "$@"
fi
