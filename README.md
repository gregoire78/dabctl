<div align="center">

# dabctl

ETI → PCM/ADTS audio pipeline for DAB/DAB+ radio, written in Rust.

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
./target/release/dabctl dablin one-service-out -i multiplex.eti -s 0xF2F8 \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -
```

Audio output on **stdout** is configurable:

- `--audio-out pcm` (default): raw signed 16-bit PCM, stereo, 48 kHz
- `--audio-out adts`: raw AAC in ADTS (no faad2/fdk decode path)

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
dabctl dablin <subcommand> [options]
```

### Subcommands

| Subcommand | Purpose |
|---|---|
| `one-service-out` | Decode one service to stdout (`pcm` or `adts`) |
| `all-services-out` | Export all DAB+ services to a directory tree |
| `list-services` | List ensemble services then exit |

### `one-service-out` options

| Flag | Short | Description | Default |
|---|---|---|---|
| `--input` | `-i` | ETI input file or `-` for stdin | required |
| `--sid` | `-s` | Service ID in hex (e.g. `0xF2F8`) | — |
| `--label` | `-l` | Select service by label instead of SID | — |
| `--audio-out` | | Stdout format: `pcm` or `adts` | `pcm` |
| `--aac-decoder` | | AAC backend: `faad2` or `fdk` (requires `fdk-aac` feature) | `faad2` |
| `--aac-gap` | | Behavior on missing/invalid AAC frames: `freeze` or `silence` | `freeze` |
| `--slide-dir` | | Save MOT slideshow images to this directory | — |
| `--slide-base64` | | Include slide payload as base64 in FD3 JSONL events | off |
| `--dedup-pad` | | Suppress consecutive identical PAD events (DL and slides) in JSONL output | off |
| `--datetime-format` | | Date/time format for `time` metadata events: preset (`human`, `iso8601`, `time-human`, `time-iso8601`) or custom chrono template | off (no `time` event) |
| `--silent` | | No log output on stderr | off |

Notes:

- `--aac-decoder` and `--aac-gap` apply only when `--audio-out pcm`.
- With `--audio-out adts`, AAC is not decoded to PCM; AUs are emitted as ADTS.

### ADTS output example

```bash
./target/release/dabctl dablin one-service-out -i multiplex.eti -s 0xF2F8 --audio-out adts \
  | ffmpeg -i - -c copy out.aac
```

### `all-services-out` options

| Flag | Short | Description | Default |
|---|---|---|---|
| `--input` | `-i` | ETI input file or `-` for stdin | required |
| `--out` | `-o` | Output directory root for all services | required |
| `--aac-decoder` | | AAC backend: `faad2` or `fdk` (requires `fdk-aac` feature) | `faad2` |
| `--aac-gap` | | Behavior on missing/invalid AAC frames: `freeze` or `silence` | `freeze` |
| `--slide-base64` | | Include slide payload as base64 in metadata files | off |
| `--dedup-pad` | | Suppress consecutive identical PAD events (DL and slides) in metadata files | off |
| `--datetime-format` | | Date/time format for `time` metadata events: preset (`human`, `iso8601`, `time-human`, `time-iso8601`) or custom chrono template | off (no `time` event) |
| `--silent` | | No log output on stderr | off |

### `list-services` options

| Flag | Short | Description | Default |
|---|---|---|---|
| `--input` | `-i` | ETI input file or `-` for stdin | required |
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
{"ensemble":{"eid":"0xf043","label":"Ile-de-France","shortLabel":"IDF"}}
{"service":{"sid":"0xf2f8","label":"NRJ"}}
{"time":{"utc":"2023-02-25, Sat - 12:34:45.321","local":"2023-02-25, Sat - 13:34:45","lto":"+01:00"}}
{"subchannel":{"id":3,"dabplus":true,"protection":"EEP-3A"}}
{"audio":{"codec":"HE-AAC","channels":2,"mode":"stereo","sampleRate":48000,"bitrate":88,"sbr":true,"ps":false}}
{"dl":"NRJ - Ed Sheeran - Shape Of You"}
{"slide":{"contentName":"cover.jpg","contentType":"image/jpeg","data":"<base64>"}}
```

Audio/profile notes:

- `subchannel.protection` comes from FIG 0/1 (for example `EEP-3A`) and `subchannel` may be emitted again if protection changes later in the stream.
- `audio.codec` is derived from DAB+ superframe signaling:
  - `AAC-LC` when `sbr=false`
  - `HE-AAC` when `sbr=true` and `ps=false`
  - `HE-AAC v2` when `sbr=true` and `ps=true`
- `audio.mode` / `audio.channels` indicate mono/stereo as decoded (e.g. stereo with `ps=false`).

`time` events are emitted only when `--datetime-format` is provided.

- `--datetime-format` omitted: no `time` event is emitted.
- `--datetime-format` provided without value: defaults to `iso8601`.

When enabled, `time` event format is configurable with `--datetime-format`:

- `utc` is always emitted as a full ISO 8601 date-time (`YYYY-MM-DDTHH:MM[:SS[.mmm]]Z`), regardless of the selected preset or custom template.
- `local` follows the selected preset or custom template.

- `human`: human-readable display format (weekday language follows system locale)
  - `local`: `YYYY-MM-DD, Ddd - HH:MM[:SS]`
  - `lto`: `+/-HH:MM`
- `iso8601` (default when `--datetime-format` has no value): machine-friendly format
  - `local`: `YYYY-MM-DDTHH:MM[:SS][+/-HH:MM]`
  - `lto`: `+/-HH:MM`
- `time-human`: human-readable time-only format
  - `local`: `HH:MM[:SS]`
- `time-iso8601`: ISO 8601 time-only format
  - `local`: `HH:MM[:SS][+/-HH:MM]`

You can also pass a custom template directly to `--datetime-format`.

Custom templates now use chrono/strftime directives.

Language for textual directives follows system locale (`LC_TIME`, then `LANG`, then `LANGUAGE`).
Locale tags are resolved with chrono locales (for example `fr_FR.UTF-8`, `de_DE.UTF-8`, `es_ES.UTF-8`). Unknown tags fall back to `POSIX`.
Abbreviated names (for example `%a`, `%b`) can include locale-specific punctuation.

- Example: `%Y-%m-%dT%H:%M:%S%:z`
- Example with literal text: `dab-time %Y-%m-%d %H:%M:%S %:z`

Note: custom templates apply to `local` only; `utc` remains ISO 8601.

Useful directives:

| Directive | Output example | Description |
|---|---|---|
| `%Y` | `2018` | Four-digit year |
| `%y` | `18` | Two-digit year |
| `%m` | `01-12` | Month, 2 digits |
| `%-m` | `1-12` | Month, no leading zero |
| `%d` | `01-31` | Day of month, 2 digits |
| `%-d` | `1-31` | Day of month, no leading zero |
| `%H` | `00-23` | Hour (24h), 2 digits |
| `%-H` | `0-23` | Hour (24h), no leading zero |
| `%I` | `01-12` | Hour (12h), 2 digits |
| `%-I` | `1-12` | Hour (12h), no leading zero |
| `%M` | `00-59` | Minute, 2 digits |
| `%-M` | `0-59` | Minute, no leading zero |
| `%S` | `00-59` | Second, 2 digits |
| `%-S` | `0-59` | Second, no leading zero |
| `%3f` | `000-999` | Milliseconds |
| `%a` | `Sun-Sat` or `Dim-Sam` | Short weekday name (localized) |
| `%A` | `Sunday-Saturday` or `Dimanche-Samedi` | Full weekday name (localized) |
| `%b` | `Jan-Dec` or `Jan-Dec` | Abbreviated month name (localized) |
| `%B` | `January-December` or `Janvier-Decembre` | Full month name (localized) |
| `%w` | `0-6` | Day of week, Sunday = 0 |
| `%:z` | `+05:00` | UTC offset, ±HH:mm |
| `%z` | `+0500` | UTC offset, ±HHmm |

> Events are only emitted once labels are fully resolved from FIG 1. No partial/label-less events are written.

---

## Examples

```bash
# Decode an ETI file → WAV
exec 3>pad_metadata.json
./target/release/dabctl dablin one-service-out \
  -i multiplex.eti -s 0xF2F8 \
  --slide-dir ./slides \
  --slide-base64 \
| ffmpeg -y -f s16le -ar 48000 -ac 2 -i pipe:0 output.wav

# Decode from stdin
cat multiplex.eti \
| ./target/release/dabctl dablin one-service-out -i - -s 0xF2F8 \
| ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# List services in an ensemble
./target/release/dabctl dablin list-services -i multiplex.eti

# Select service by label
./target/release/dabctl dablin one-service-out -i multiplex.eti -l "NRJ" \
| ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# FDK-AAC backend + silence gap fill + slideshow
exec 3>pad_metadata.json
./target/release/dabctl dablin one-service-out \
  -i multiplex.eti -s 0xF2F8 \
  --aac-decoder fdk \
  --aac-gap silence \
  --datetime-format time-iso8601 \
  --slide-dir ./slides \
  --slide-base64 \
| aplay -f S16_LE -r 48000 -c 2

# Export all DAB+ services from the ETI into one directory tree
./target/release/dabctl dablin all-services-out \
  -i multiplex.eti \
  --out ./all-services \
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
    fic/mod.rs             FIC/FIG decoder (FIG 0/0, 0/1, 0/2, 0/9, 0/10, 0/13, 1/0, 1/1, 1/5)
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
