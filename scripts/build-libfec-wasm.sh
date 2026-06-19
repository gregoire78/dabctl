#!/usr/bin/env bash
set -euo pipefail

# Build a static wasm32 libfec archive (libfec.a) from C sources.
#
# Default usage (uses vendored sources):
#   scripts/build-libfec-wasm.sh
#
# Custom usage:
#   scripts/build-libfec-wasm.sh <libfec_source_dir> <output_dir>
#
# Output is placed in third_party/libfec/wasm/libfec.a by default.
# After building, set LIBFEC_WASM_LIB_DIR=third_party/libfec/wasm for cargo.
#
# Requirements:
#   clang (>= 14 with wasm32 target), llvm-ar
#   apt: wasi-libc (for C stdlib headers)
#   apt: sudo apt-get install -y clang llvm wasi-libc

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SRC_DIR="${1:-$REPO_ROOT/third_party/libfec/src}"
OUT_DIR="${2:-$REPO_ROOT/third_party/libfec/wasm}"
OBJ_DIR="$OUT_DIR/obj"

# Only the RS char functions are required by dabctl.
RS_CFILES=(
  init_rs_char.c
  decode_rs_char.c
  init_rs_char_local.c
)

# WASI sysroot for C standard headers (stdlib.h, string.h, etc.)
WASI_SYSROOT="${WASI_SYSROOT:-/usr/include/wasm32-wasi}"

if [[ ! -d "$SRC_DIR" ]]; then
  echo "error: libfec source directory not found: $SRC_DIR" >&2
  echo "hint:  git clone --depth=1 https://github.com/fblomqvi/libfec $SRC_DIR" >&2
  exit 1
fi

if [[ ! -d "$WASI_SYSROOT" ]]; then
  echo "error: wasi sysroot not found: $WASI_SYSROOT" >&2
  echo "hint:  sudo apt-get install -y wasi-libc" >&2
  exit 1
fi

mkdir -p "$OBJ_DIR"

for cfile in "${RS_CFILES[@]}"; do
  src_path="$SRC_DIR/$cfile"
  if [[ ! -f "$src_path" ]]; then
    echo "error: missing source file: $src_path" >&2
    exit 1
  fi
  obj_path="$OBJ_DIR/${cfile%.c}.o"
  clang \
    --target=wasm32-unknown-unknown \
    -Oz \
    -fvisibility=hidden \
    -fno-builtin \
    -isystem "$WASI_SYSROOT" \
    -I "$SRC_DIR" \
    -c "$src_path" \
    -o "$obj_path" && echo "  compiled: $cfile"
done

llvm-ar rcs "$OUT_DIR/libfec.a" "$OBJ_DIR"/*.o

echo "==> built: $OUT_DIR/libfec.a"
echo ""
echo "To build dabctl wasm:"
echo "  LIBFEC_WASM_LIB_DIR=$OUT_DIR cargo build --target wasm32-unknown-unknown --features wasm-runtime"
