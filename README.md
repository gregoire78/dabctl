<div align="center">

# dabctl

**RTL-SDR → PCM audio pipeline for DAB+ radio, written in Rust**

Rust port of [eti-cmdline](https://github.com/JvanKatwijk/eti-stuff/tree/master/eti-cmdline) (IQ → ETI)
and [dablin](https://github.com/Opendigitalradio/dablin) (ETI → audio),
unified into a single RTL-SDR → PCM direct pipeline.

[![Rust](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org/)
[![License: GPL-2.0](https://img.shields.io/badge/License-GPL%202.0-blue.svg)](COPYING)

</div>

---

## Quick start

```bash
# 1. Install dependencies (Debian/Ubuntu)
sudo apt install -y libusb-1.0-0-dev pkg-config build-essential libfaad-dev

# 2. Build
cargo build --release

# 3. Listen — channel 6C, service NRJ (SID 0xF2F8)
sudo ./target/release/dabctl -C 6C -s 0xF2F8 \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -
```

Audio output is **signed 16-bit PCM, stereo, 48 kHz** on stdout.
Only **DAB+** services (HE-AAC) are decoded; classic DAB (MP2) is not supported.

---

## Prerequisites

### System packages

| Package | Role |
|---|---|
| `libusb-1.0-0-dev` | USB backend for RTL-SDR |
| `pkg-config` | Library discovery |
| `build-essential` | C compiler (required by libfaad2 / libfdk-aac) |
| `libfaad-dev` | AAC decoder for DAB+ (default backend) |
| `libfdk-aac-dev` | Alternative AAC decoder — Fraunhofer FDK (optional, `fdk-aac` feature) |

> `libfdk-aac-dev` is in the `non-free` component on Debian (Fraunhofer audio patents).
> Use the default **faad2** backend unless FDK-AAC quality is specifically required.

```bash
# faad2 backend (default)
sudo apt install -y libusb-1.0-0-dev pkg-config build-essential libfaad-dev

# fdk-aac backend — enable non-free first on Debian Trixie
sudo sed -i 's/Components: main$/Components: main non-free/' /etc/apt/sources.list.d/debian.sources
sudo apt update && sudo apt install -y libusb-1.0-0-dev pkg-config build-essential libfdk-aac-dev
```

### Hardware

- RTL-SDR dongle (RTL2832U / R820T2 chipset)
- DAB Band III antenna (174–240 MHz)

### Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

---

## Building

The RTL-SDR driver is handled entirely by the
[`rtl-sdr-rs`](https://github.com/ccostes/rtl-sdr-rs) crate (pure Rust, no CMake or
`libclang` required). Only `libusb-1.0` must be present.

```bash
cargo build --release                        # faad2 backend (default)
cargo build --release --features fdk-aac    # Fraunhofer FDK-AAC backend
```

With `--features fdk-aac`, both libraries are linked and the backend is selected at
runtime via `--aac-decoder`.

### Dev Container

A ready-to-use devcontainer is provided for VS Code and GitHub Codespaces
(`.devcontainer/devcontainer.json`). It installs all system dependencies on creation.

1. Install the **Dev Containers** extension in VS Code.
2. **Ctrl+Shift+P** → `Dev Containers: Reopen in Container`.
3. `cargo build --release`.

---

## CLI reference

```
dabctl -C <channel> -s <sid> [options]
```

| Option | Short | Description | Default |
|---|---|---|---|
| `--channel` | `-C` | DAB channel (e.g. `5A`, `6C`, `11C`) | **required** |
| `--sid` | `-s` | Service ID in hex (e.g. `0xF2F8`) | **required** |
| `--gain` | `-G` | Tuner gain in % (0–100) | software AGC |
| `--ppm` | `-p` | Frequency correction in PPM | `0` |
| `--sync-time` | `-d` | Sync timeout in seconds | `5` |
| `--detect-time` | `-D` | Ensemble detection timeout in seconds | `10` |
| `--label` | `-l` | Select service by label instead of SID | — |
| `--disable-dyn-fic` | `-F` | Suppress FIC log messages on stderr | off |
| `--slide-dir` | `-S` | Save slideshow images to this directory | — |
| `--slide-base64` | | Include slideshow images as base64 in JSON output | off |
| `--silent` | | No log output on stderr | off |
| `--device-index` | | RTL-SDR dongle index | `0` |
| `--aac-decoder` | | AAC backend: `faad2` or `fdk-aac` (requires `fdk-aac` feature) | `faad2` |

Band III channels span **5A–13F** (174.928–239.200 MHz).
L-Band channels (LA–LP, 1452–1478 MHz) are also supported.

### Metadata output (fd 3)

JSON events are emitted one per line on file descriptor 3:

```json
{"ensemble":{"eid":"0x1000","label":"DAB+ France"}}
{"service":{"sid":"0xF2F8","label":"NRJ"}}
{"dl":"NRJ - Ed Sheeran - Shape Of You"}
{"slide":{"contentName":"cover.jpg","contentType":"image/jpeg","data":"<base64>"}}
```

Redirect with: `3>metadata.json`

---

## Examples

```bash
# Software AGC (default)
sudo ./target/release/dabctl -C 6C -s 0xF2F8 \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# Manual gain + frequency correction
sudo ./target/release/dabctl -C 6C -s 0xF2F8 -G 20 -p 2 \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# aplay instead of ffplay
sudo ./target/release/dabctl -C 11C -s 0xF2F8 -G 50 \
  | aplay -f S16_LE -r 48000 -c 2

# Capture slideshow and DLS metadata
sudo ./target/release/dabctl -C 6C -s 0xF2F8 \
  --slide-dir /tmp/slides --slide-base64 3>pad_metadata.json \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# Convert to WAV
sudo ./target/release/dabctl -C 6C -s 0xF2F8 -G 20 \
  | sox -t raw -r 48000 -b 16 -c 2 -e signed-integer -L - output.wav

# Automated capture helper script
./live-capture-iq2pcm.sh 6C 0xF2F8 20
```

---

## Architecture

### Source tree

```
build.rs                        Linker directives for faad2 / fdk-aac
src/
  main.rs                       CLI entry point (clap) → pipeline
  lib.rs                        Module declarations
  iq2pcm_cmd.rs                 Main pipeline: RTL-SDR → PCM
  device/
    rtlsdr_handler.rs           RTL-SDR via rtl-sdr-rs
  pipeline/
    dab_constants.rs            Constants, CRC helpers, bit utilities
    dab_frame.rs                DabFrame — in-process FIC + subchannel transport
    dab_params.rs               DAB Mode I–IV parameters (ETSI EN 300 401 §14)
    band_handler.rs             Channel name → centre frequency
    ringbuffer.rs               Thread-safe IQ ring buffer (SPSC)
    subchannel_pool.rs          Pre-allocated subchannel buffer pool
    eti_generator.rs            DabPipeline: OFDM blocks → DabFrame via mpsc
    fib_processor.rs            FIG 0/0, 0/1, FIG 1 parsing
    fic_handler.rs              FIC depuncturing and Viterbi decoding
    viterbi_handler.rs          Viterbi decoder {0155,0117,0123,0155}
    prot_tables.rs              24 puncturing tables (ETSI EN 300 401 §11)
    protection.rs               EEP + UEP deconvolution
    cif_interleaver.rs          CIF 16-frame time interleaving
    ofdm/
      phase_table.rs            Mode I phase reference table
      phase_reference.rs        Sync correlation + coarse frequency offset
      freq_interleaver.rs       Carrier frequency interleaving
      ofdm_processor.rs         Main OFDM demodulation loop
  audio/
    fic_decoder.rs              FIC/FIG service discovery (FIG 0/0, 0/1, 0/2, 1/0, 1/1)
    superframe.rs               DAB+ superframe: 5 CIFs → Reed-Solomon FEC → AUs
    rs_decoder.rs               Reed-Solomon (120,110) over GF(2^8), pure Rust
    aac_decoder/
      mod.rs                    AAC decoder trait + runtime backend selection
      faad2.rs                  FFI binding to libfaad2
      fdkaac.rs                 FFI binding to libfdk-aac
    pad_decoder.rs              F-PAD + X-PAD, DLS (Dynamic Label), MOT slideshow
    pad_output.rs               JSON metadata + slideshow output on fd 3
    mot_decoder.rs              X-PAD → MOT DataGroups (accumulation + CRC)
    mot_manager.rs              MOT DataGroups → complete MOT object (JPEG/PNG)
    crc.rs                      CRC-16-CCITT and Fire Code
    ebu_latin.rs                EBU Latin-1 → UTF-8 (ETSI EN 300 401 §8.1.1.1)
```

### Thread model

1. **OFDM thread** — reads IQ samples from the RTL-SDR dongle, performs FFT, symbol
   synchronisation, and frequency interleaving, and sends OFDM blocks to `DabPipeline`
   via a pre-allocated SPSC ring buffer.

2. **DabPipeline thread** — receives OFDM blocks, decodes the FIC (Fast Information
   Channel), applies CIF time interleaving, and runs Viterbi deconvolution on each
   subchannel. Emits one `DabFrame` per CIF over a bounded `mpsc` channel (capacity
   4 frames ≈ 100 ms of back-pressure).

3. **Main thread** — consumes `DabFrame` objects, feeds `FicDecoder` for service
   discovery, drives `SuperframeFilter` → `AacDecoder` for HE-AAC audio, and
   `PadDecoder` → `PadOutput` for DLS/MOT metadata.

### Data flow

```
RTL-SDR (2.048 MHz IQ)
  └─ RtlsdrHandler (USB → f32 IQ)
       └─ OfdmProcessor (FFT, symbol sync, carrier interleaving)
            └─ DabPipeline (FIC + CIF → DabFrame)
                 ├─ FicDecoder (service discovery via FIG parsing)
                 └─ SuperframeFilter (5 CIFs → RS FEC → Access Units)
                      ├─ AacDecoder (HE-AAC) → 16-bit PCM on stdout
                      └─ PadDecoder (DLS + MOT slideshow)
                           └─ PadOutput (JSON events on fd 3)
```

---

## Cross-compilation

### ARM64 (Raspberry Pi 3/4, 64-bit)

```bash
rustup target add aarch64-unknown-linux-gnu
sudo apt install -y gcc-aarch64-linux-gnu libc6-dev-arm64-cross
cargo build --release --target aarch64-unknown-linux-gnu
scp target/aarch64-unknown-linux-gnu/release/dabctl user@rpi:/usr/local/bin/
```

### ARM32 (Raspberry Pi 2/3, 32-bit)

```bash
rustup target add armv7-unknown-linux-gnueabihf
sudo apt install -y gcc-arm-linux-gnueabihf
cargo build --release --target armv7-unknown-linux-gnueabihf
```

Add linker entries to `.cargo/config.toml`:

```toml
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"

[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
```

On the target device, install `libusb-1.0-0` and `libfaad2` (or `libfdk-aac2`), then
copy the binary to `/usr/local/bin/`.

---

## Troubleshooting

**`No RTL-SDR devices found`**

The kernel DVB driver claims the device before `dabctl` can open it. Blacklist it:

```bash
sudo rmmod dvb_usb_rtl28xxu 2>/dev/null
echo "blacklist dvb_usb_rtl28xxu" | sudo tee /etc/modprobe.d/rtlsdr.conf
```

**No sync / weak signal**

Try a higher fixed gain (`-G 80`) or omit `-G` entirely to let software AGC adapt.
Check that the antenna is connected and oriented vertically for Band III.

**Garbled audio / decoder errors**

Try the alternative AAC backend (requires a `--features fdk-aac` build):

```bash
sudo ./target/release/dabctl -C 6C -s 0xF2F8 --aac-decoder fdk-aac | ffplay …
```

---

## Man page

```bash
man ./dabctl.1                                          # view locally

sudo install -m 644 dabctl.1 /usr/local/share/man/man1/
sudo mandb
man dabctl
```

---

## References

### Upstream projects

| Project | Role |
|---|---|
| [eti-cmdline](https://github.com/JvanKatwijk/eti-stuff/tree/master/eti-cmdline) | Reference C++ IQ → ETI implementation — base for the signal processing chain |
| [dablin](https://github.com/Opendigitalradio/dablin) | Reference C++ ETI → audio decoder — base for the audio pipeline |
| [AbracaDABra](https://github.com/KejPi/AbracaDABra) | Software AGC and AAC backend selection strategy (MIT licence) |
| [rtl-sdr-rs](https://github.com/ccostes/rtl-sdr-rs) | Pure-Rust RTL-SDR driver (via `rusb`) |
| [osmocom/rtl-sdr](https://github.com/osmocom/rtl-sdr) | Original RTL-SDR C library |

### ETSI standards

| Standard | Title |
|---|---|
| [ETSI EN 300 401](https://www.etsi.org/deliver/etsi_en/300400_300499/300401/02.01.01_60/en_300401v020101p.pdf) | Radio Broadcasting Systems; Digital Audio Broadcasting — core system spec (OFDM, FIC, CIF, protection) |
| [ETSI TS 102 563](https://www.etsi.org/deliver/etsi_ts/102500_102599/102563/02.01.01_60/ts_102563v020101p.pdf) | DAB+ audio coding (HE-AAC v2) |
| [ETSI TS 103 466](https://www.etsi.org/deliver/etsi_ts/103400_103499/103466/01.02.01_60/ts_103466v010201p.pdf) | DAB audio coding (MPEG-1 Layer II) |
| [ETSI TS 101 756](https://www.etsi.org/deliver/etsi_ts/101700_101799/101756/02.04.01_60/ts_101756v020401p.pdf) | Registered tables (SId, language codes, country codes, service types) |
| [ETSI ETS 300 799](https://www.etsi.org/deliver/etsi_i_ets/300700_300799/300799/01_60/ets_300799e01p.pdf) | Ensemble Transport Interface (ETI-NI) |
| [ETSI EN 301 234](https://www.etsi.org/deliver/etsi_en/301200_301299/301234/02.01.01_60/en_301234v020101p.pdf) | Multimedia Object Transfer (MOT) protocol |
| [ETSI TS 101 499](https://www.etsi.org/deliver/etsi_ts/101400_101499/101499/03.01.01_60/ts_101499v030101p.pdf) | MOT Slideshow application |
| [ETSI TS 102 980](https://www.etsi.org/deliver/etsi_ts/102900_102999/102980/02.01.02_60/ts_102980v020102p.pdf) | Dynamic Label Plus (DL+) |
