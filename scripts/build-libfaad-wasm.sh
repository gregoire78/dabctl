#!/usr/bin/env bash
set -euo pipefail

# Build a static wasm32 libfaad archive (libfaad.a) from C sources.
#
# Default usage (uses vendored or cloned sources):
#   scripts/build-libfaad-wasm.sh
#
# Custom usage:
#   scripts/build-libfaad-wasm.sh <faad2_source_dir> <output_dir>
#
# The faad2 source directory must contain a libfaad/ subdirectory.
# Clone it once with:
#   git clone --depth=1 https://github.com/knik0/faad2 third_party/libfaad/src
#
# Output is placed in third_party/libfaad/wasm/libfaad.a by default.
# After building, the cargo build will pick it up automatically, or set:
#   LIBFAAD_WASM_LIB_DIR=third_party/libfaad/wasm
#
# Requirements:
#   clang (>= 14 with wasm32 target), llvm-ar
#   apt: wasi-libc  (sudo apt-get install -y clang llvm wasi-libc)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SRC_DIR="${1:-$REPO_ROOT/third_party/libfaad/src}"
OUT_DIR="${2:-$REPO_ROOT/third_party/libfaad/wasm}"
OBJ_DIR="$OUT_DIR/obj"

LIBFAAD_SRC="$SRC_DIR/libfaad"

# WASI sysroot for C standard headers (stdlib.h, string.h, math.h …)
WASI_SYSROOT="${WASI_SYSROOT:-/usr/include/wasm32-wasi}"

if [[ ! -d "$SRC_DIR" ]]; then
  echo "error: faad2 source directory not found: $SRC_DIR" >&2
  echo "hint:  git clone --depth=1 https://github.com/knik0/faad2 $SRC_DIR" >&2
  exit 1
fi

if [[ ! -d "$LIBFAAD_SRC" ]]; then
  echo "error: libfaad subdirectory not found inside $SRC_DIR" >&2
  echo "hint:  ensure the cloned repo contains a libfaad/ directory" >&2
  exit 1
fi

if [[ ! -d "$WASI_SYSROOT" ]]; then
  echo "error: wasi sysroot not found: $WASI_SYSROOT" >&2
  echo "hint:  sudo apt-get install -y wasi-libc" >&2
  exit 1
fi

mkdir -p "$OBJ_DIR"

# Collect all portable C files from libfaad/.
# Exclude assembly/platform stubs that would break wasm compilation.
CFILES=()
while IFS= read -r -d '' f; do
  CFILES+=("$f")
done < <(find "$LIBFAAD_SRC" -maxdepth 1 -name "*.c" -print0 | sort -z)

if [[ ${#CFILES[@]} -eq 0 ]]; then
  echo "error: no .c files found in $LIBFAAD_SRC" >&2
  exit 1
fi

echo "==> compiling ${#CFILES[@]} source files to wasm32-unknown-unknown …"

# Create a minimal shim directory to override the WASI-platform-only guard in
# <wasi/api.h> which the WASI sysroot's stdio.h transitively includes.
# faad2 includes <stdio.h> unconditionally, triggering the guard on non-WASI targets.
SHIM_DIR="$OBJ_DIR/include-shim"
mkdir -p "$SHIM_DIR/wasi"
printf '/* wasm32-unknown-unknown stub: suppress WASI-only guard */\n' \
  > "$SHIM_DIR/wasi/api.h"

# Build a tiny libc shim for wasm32-unknown-unknown where no C runtime
# is linked by default. libfaad references a small subset of libc symbols.
# Keep the generated source alongside the other build objects.
LIBC_SHIM_C="$OBJ_DIR/libc-shim.c"
cat > "$LIBC_SHIM_C" <<'EOF'
#include <stddef.h>
#include <stdarg.h>

typedef struct FakeFile FILE;
FILE *stderr = (FILE *)0;

int fprintf(FILE *stream, const char *fmt, ...) {
  (void)stream;
  (void)fmt;
  return 0;
}

int abs(int x) {
  return x < 0 ? -x : x;
}

void __assert_fail(const char *assertion, const char *file, unsigned int line, const char *function) {
  (void)assertion;
  (void)file;
  (void)line;
  (void)function;
  __builtin_trap();
}

static void swap_bytes(unsigned char *a, unsigned char *b, size_t n) {
  for (size_t i = 0; i < n; ++i) {
    unsigned char t = a[i];
    a[i] = b[i];
    b[i] = t;
  }
}

void qsort(void *base, size_t nmemb, size_t size, int (*compar)(const void *, const void *)) {
  if (base == NULL || compar == NULL || size == 0 || nmemb < 2) {
    return;
  }

  unsigned char *arr = (unsigned char *)base;
  for (size_t i = 1; i < nmemb; ++i) {
    size_t j = i;
    while (j > 0) {
      unsigned char *left = arr + (j - 1) * size;
      unsigned char *right = arr + j * size;
      if (compar(left, right) <= 0) {
        break;
      }
      swap_bytes(left, right, size);
      --j;
    }
  }
}
EOF

OBJECTS=()
for src_path in "${CFILES[@]}"; do
  cfile="$(basename "$src_path")"
  obj_path="$OBJ_DIR/${cfile%.c}.o"
  clang \
    --target=wasm32-unknown-unknown \
    -Oz \
    -fvisibility=hidden \
    -fno-builtin \
    -fno-exceptions \
    -I "$SHIM_DIR" \
    -isystem "$WASI_SYSROOT" \
    -I "$LIBFAAD_SRC" \
    -I "$SRC_DIR/include" \
    -DHAVE_STDINT_H=1 \
    -DHAVE_STRING_H=1 \
    -DHAVE_MEMCPY=1 \
    -DWORDS_BIGENDIAN=0 \
      "-DPACKAGE_VERSION=\"2.0\"" \
      -c "$src_path" \
    -o "$obj_path" && echo "  compiled: $cfile"
  OBJECTS+=("$obj_path")
done

# Compile libc shim object and append it to the static archive.
LIBC_SHIM_O="$OBJ_DIR/libc-shim.o"
clang \
  --target=wasm32-unknown-unknown \
  -Oz \
  -fvisibility=hidden \
  -fno-builtin \
  -fno-exceptions \
  -I "$SHIM_DIR" \
  -isystem "$WASI_SYSROOT" \
  -c "$LIBC_SHIM_C" \
  -o "$LIBC_SHIM_O" && echo "  compiled: libc-shim.c"
OBJECTS+=("$LIBC_SHIM_O")

llvm-ar rcs "$OUT_DIR/libfaad.a" "${OBJECTS[@]}"

echo ""
echo "==> built: $OUT_DIR/libfaad.a"
echo ""
echo "To build dabctl wasm with faad2 PCM decoding:"
echo "  LIBFEC_WASM_LIB_DIR=third_party/libfec/wasm \\"
echo "  LIBFAAD_WASM_LIB_DIR=$OUT_DIR \\"
echo "  cargo build --target wasm32-unknown-unknown --features wasm-faad2"
