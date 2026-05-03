<div align="center">

# dabctl

ETI → PCM audio pipeline for DAB/DAB+ radio, written in Rust.

Adapted from [gregoire78/dablin](https://github.com/gregoire78/dablin) — integrated as a strict CLI subcommand.

[![Rust](https://img.shields.io/badge/rust-2021-orange)](https://www.rust-lang.org/)
[![License: GPL-2.0](https://img.shields.io/badge/License-GPL--2.0-blue)](LICENSE)

</div>

---

## Quick start

```bash
# 1. Install dependencies (Debian/Ubuntu)
sudo apt install -y build-essential pkg-config libfaad-dev

# Optional: Fraunhofer FDK-AAC backend (non-free)
sudo sed -i 's/Components: main$/Components: main non-free/' /etc/apt/sources.list.d/debian.sources
sudo apt update && sudo apt install -y libfdk-aac-dev

# 2. Build
cargo build --release

# 3. Decode an ETI file — service NRJ (SID 0xF2F8)
./target/release/dabctl dablin -i multiplex.eti -s 0xF2F8 \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -
```

Audio output is raw signed 16-bit PCM, stereo, 48 kHz on **stdout**.

---

## Prerequisites

### System packages

| Package | Purpose |
|---|---|
| `build-essential` | C compiler (required by libfaad2 / libfdk-aac) |
| `pkg-config` | Library discovery |
| `libfaad-dev` | AAC decoder for DAB+ (default backend) |
| `libfdk-aac-dev` | Alternative AAC decoder — Fraunhofer FDK (optional, `fdk-aac` feature) |

> `libfdk-aac-dev` is in the `non-free` component on Debian (Fraunhofer audio patents). Use the default faad2 backend unless FDK-AAC quality is specifically required.

### Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

---

## Building

```bash
# Default: faad2 backend
cargo build --release

# With Fraunhofer FDK-AAC backend
cargo build --release --features fdk-aac
```

### Dev Container

A ready-to-use devcontainer is provided for VS Code and GitHub Codespaces (`.devcontainer/`).

1. Install the **Dev Containers** extension in VS Code.
2. `Ctrl+Shift+P` → `Dev Containers: Reopen in Container`.
3. `cargo build --release`.

---

## CLI reference

```
dabctl dablin -i <eti-file|-> -s <sid> [options]
```

### Options

| Flag | Short | Description | Default |
|---|---|---|---|
| `--input` | `-i` | ETI input file or `-` for stdin | required |
| `--sid` | `-s` | Service ID in hex (e.g. `0xF2F8`) | — |
| `--label` | `-l` | Select service by label instead of SID | — |
| `--list-services` | | List ensemble services then exit | off |
| `--aac-decoder` | | AAC backend: `faad2` or `fdk` (requires `fdk-aac` feature) | `faad2` |
| `--aac-gap` | | Behavior on missing/invalid AAC frames: `freeze` or `silence` | `freeze` |
| `--slide-dir` | | Save MOT slideshow images to this directory | — |
| `--slide-base64` | | Include slide payload as base64 in FD3 JSONL events | off |
| `--all-services-out` | | Export all DAB+ services to per-service folders (`audio.wav`, `slides/`, `metadata.jsonl`) | — |
| `--dedup-pad` | | Suppress consecutive identical PAD events (DL and slides) in JSONL output | off |
| `--silent` | | No log output on stderr | off |

### AAC gap policy

| Policy | Behavior |
|---|---|
| `freeze` (default) | No PCM output on decode error — legacy behavior |
| `silence` | Emit PCM silence (1024 × channels zero samples) to keep the stream alive |

---

## Metadata output (FD 3)

JSONL events are emitted one per line on **file descriptor 3**.  
Open it with a shell redirect: `3>metadata.jsonl`

```jsonl
{"ensemble":{"eid":"0xf043","label":"Ile-de-France"}}
{"service":{"sid":"0xf2f8","label":"NRJ"}}
{"bitrate":88}
{"dl":"NRJ - Ed Sheeran - Shape Of You"}
{"slide":{"contentName":"cover.jpg","contentType":"image/jpeg","data":"<base64>"}}
```

> Events are only emitted once labels are fully resolved from FIG 1. No partial/label-less events are written.

---

## Examples

```bash
# Decode an ETI file → WAV
exec 3>pad_metadata.json
./target/release/dabctl dablin \
  -i multiplex.eti -s 0xF2F8 \
  --slide-dir ./slides \
  --slide-base64 \
| ffmpeg -y -f s16le -ar 48000 -ac 2 -i pipe:0 output.wav

# Decode from stdin
cat multiplex.eti \
| ./target/release/dabctl dablin -i - -s 0xF2F8 \
| ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# List services in an ensemble
./target/release/dabctl dablin -i multiplex.eti --list-services

# Select service by label
./target/release/dabctl dablin -i multiplex.eti -l "NRJ" \
| ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# FDK-AAC backend + silence gap fill + slideshow
exec 3>pad_metadata.json
./target/release/dabctl dablin \
  -i multiplex.eti -s 0xF2F8 \
  --aac-decoder fdk \
  --aac-gap silence \
  --slide-dir ./slides \
  --slide-base64 \
| aplay -f S16_LE -r 48000 -c 2

# Export all DAB+ services from the ETI into one directory tree
./target/release/dabctl dablin \
  -i multiplex.eti \
  --all-services-out ./all-services \
  --slide-base64
# Produces:
# ./all-services/global-index.jsonl
# ./all-services/0xf2f8-NRJ/audio.wav
# ./all-services/0xf2f8-NRJ/metadata.jsonl
# ./all-services/0xf2f8-NRJ/slides/

# Capture helper script (builds, tests, decodes, saves WAV)
bash live-capture-iq2pcm.sh multiplex.eti 0xF2F8
```

---

## Output contract

| Stream | Content |
|---|---|
| **stdout** | Raw PCM — `s16le`, 48 kHz, stereo, no headers |
| **stderr** | Logs only (tracing) — fully suppressed with `--silent` |
| **FD 3** | JSONL metadata — one event per line |

---

## Architecture

### Processing pipeline

```
ETI (file / stdin)
  └─ ETI-NI frame parser
       └─ FIC / FIG decoder  (ensemble, service, labels)
            └─ MSC sub-channel extractor
                 └─ DAB+ superframe assembler (5 CIFs)
                      └─ Reed-Solomon FEC (120,110)
                           └─ FireCode sync
                                └─ AAC decoder (faad2 / fdk-aac)
                                     ├─ PCM → stdout (s16le 48 kHz stereo)
                                     └─ PAD decoder (DLS + MOT slideshow)
                                          └─ JSONL events → FD 3
```

### Source tree

```
src/
  main.rs                  CLI entry point
  cli.rs                   clap argument definitions
  dablin/
    mod.rs
    runner.rs              Main decoding loop
    metadata.rs            JSONL emitter (FD 3)
    eti/mod.rs             ETI-NI frame parser + FSYNC
    fic/mod.rs             FIC/FIG decoder (FIG 0/0, 0/1, 0/2, 0/13, 1/0, 1/1, 1/5)
    msc/mod.rs             MSC sub-channel extraction
    dabplus/
      mod.rs               DAB+ superframe assembly
      firecode.rs          FireCode CRC-16 sync
      rs_decoder.rs        Reed-Solomon (120,110) pure Rust
    audio/
      mod.rs               AacDecoder wrapper + gap policy
      faad2.rs             FFI → libfaad2
      fdk.rs               FFI → libfdk-aac (feature-gated)
    pad/mod.rs             F-PAD / X-PAD, DLS, MOT slideshow
    utils/
      ebu_latin.rs         EBU Latin-1 → UTF-8 (ETSI EN 300 401 §8.1.1.1)
      jsonl.rs             JSONL writer helper for metadata files
      path.rs              Path-safe label sanitization helper
      wav_writer.rs        WAV file writer for all-services export
```

---

## References

### Upstream projects

| Project | Role |
|---|---|
| [gregoire78/dablin](https://github.com/gregoire78/dablin) | Reference ETI → audio decoder — sole authoritative reference |
| [Opendigitalradio/dablin](https://github.com/Opendigitalradio/dablin) | Upstream dablin (upstream only if behavior matches the fork exactly) |

### ETSI standards

| Standard | Description |
|---|---|
| ETSI EN 300 401 | Digital Audio Broadcasting — core system spec (FIC, CIF, protection) |
| ETSI TS 102 563 | DAB+ audio coding (HE-AAC v2) |
| ETSI ETS 300 799 | Ensemble Transport Interface (ETI-NI) |
| ETSI EN 301 234 | Multimedia Object Transfer (MOT) protocol |
| ETSI TS 101 499 | MOT Slideshow application |
| ETSI TS 102 980 | Dynamic Label Plus (DL+) |

---

## Licence

`dabctl` is released under the **GNU General Public License v2.0** (GPL-2.0).  
See the [LICENSE](LICENSE) file for the full licence text.

| Dependency | Licence |
|---|---|
| dablin (gregoire78 fork) | GPL-2.0 |
| libfaad2 | GPL-2.0 |
| libfdk-aac (optional) | Fraunhofer FDK AAC Codec Library Licence (non-free) |
