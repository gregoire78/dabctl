<div align="center">

# 📡 eti-rtlsdr-rust

**Réception DAB complète en Rust : RTL-SDR → ETI → PCM audio**

Port Rust de [eti-cmdline](https://github.com/JvanKatwijk/eti-stuff) (IQ → ETI) et [dablin](https://github.com/Opendigitalradio/dablin) (ETI → audio).

[![Rust](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org/)
[![License: GPL-2.0](https://img.shields.io/badge/License-GPL%202.0-blue.svg)](COPYING)

</div>

---

## ⚡ Démarrage rapide

```bash
# 1. Build
cargo build --release

# 2. Configurer LD_LIBRARY_PATH
export LD_LIBRARY_PATH=$(find target -name "librtlsdr.so.0" 2>/dev/null | head -1 | xargs dirname):$LD_LIBRARY_PATH

# 3. Pipeline complète : RTL-SDR → ETI → PCM → lecteur audio
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 \
  | ./target/release/eti-rtlsdr-rust eti2pcm -F -s 0xF2F8 -p \
  | ffplay -f s16le -ar 48000 -ac 2 -i -

# 4. Ou plus simplement, avec le script helper :
./eti-rtlsdr-rust.sh -S -C 11C -G 80 | dablin_gtk -L
```

---

## 📋 Prérequis

### Système

| Paquet | Rôle |
|---|---|
| `cmake` | Build de librtlsdr |
| `libusb-1.0-0-dev` | USB backend pour RTL-SDR |
| `pkg-config` | Découverte de libs |
| `build-essential` | Compilateur C |
| `clang`, `libclang-dev` | Requis par bindgen |
| `libfaad-dev` | Décodeur AAC pour DAB+ (`eti2pcm`) |
| `libmpg123-dev` | Décodeur MP2 pour DAB classique (`eti2pcm`) |

```bash
sudo apt install -y cmake libusb-1.0-0-dev pkg-config build-essential clang libclang-dev libfaad-dev libmpg123-dev
```

### Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Bibliothèques liées

| Bibliothèque | Version | Rôle | Lien |
|---|---|---|---|
| **librtlsdr** | 0.6+ | Pilote RTL-SDR (compilé automatiquement via `build.rs`) | [osmocom/rtl-sdr](https://github.com/osmocom/rtl-sdr) |
| **libusb-1.0** | 1.0+ | Backend USB pour librtlsdr | [libusb.info](https://libusb.info) |
| **libfaad2** | 2.11+ | Décodeur AAC (DAB+) pour `eti2pcm` | [knik-o/faad2](https://github.com/knik-o/faad2) |
| **libmpg123** | 1.32+ | Décodeur MP2 (DAB classique) pour `eti2pcm` | [mpg123.de](https://mpg123.de) |

> **Note :** `librtlsdr` est incluse dans le dépôt (`rtl-sdr/`) et compilée statiquement par `build.rs` via CMake. Seule `libusb` doit être installée sur le système. `libfaad2` et `libmpg123` sont nécessaires uniquement pour la sous-commande `eti2pcm`.

### Crates Rust

| Crate | Version | Rôle |
|---|---|---|
| `clap` | 4.4 | Parsing des arguments CLI (sous-commandes) |
| `rustfft` | 6.4 | FFT pour démodulation OFDM |
| `num-complex` | 0.4 | Types complexes (IQ) |
| `rayon` | 1.10 | Parallélisation des sous-canaux |
| `tracing` | 0.1 | Logging structuré |
| `tracing-subscriber` | 0.3 | Formatage et filtrage des logs |
| `anyhow` | 1.0 | Gestion d'erreurs |
| `ctrlc` | 3.4 | Handler Ctrl-C |
| `libc` | 0.2 | Écriture JSON sur fd 3 (`eti2pcm`) |
| `serde` | 1.0 | Sérialisation (`eti2pcm`) |
| `serde_json` | 1.0 | Sortie JSON métadonnées DAB (`eti2pcm`) |
| `base64` | 0.22 | Encodage base64 des images slideshow (`eti2pcm`) |
| `bindgen` | 0.69 | Génération FFI C → Rust (build) |
| `cmake` | 0.1 | Compilation librtlsdr (build) |

### Matériel

- Dongle RTL-SDR (RTL2832U / R820T2)
- Antenne DAB (bande III, 174–240 MHz)

---

## 🐳 Dev Container

Le projet inclut un devcontainer prêt à l'emploi pour VS Code / GitHub Codespaces.

```jsonc
// .devcontainer/devcontainer.json
{
  "image": "mcr.microsoft.com/devcontainers/rust:2-1-trixie",
  "features": {
    "ghcr.io/devcontainers-extra/features/apt-packages:1": {
      "packages": "cmake pkg-config build-essential libusb-1.0-0-dev clang libclang-dev llvm-dev gcc-aarch64-linux-gnu"
    }
  },
  "privileged": true  // accès USB pour RTL-SDR
}
```

Le `postCreateCommand` installe automatiquement les targets de cross-compilation.

Ouvrir le projet :
1. Installer l'extension **Dev Containers** dans VS Code
2. Ctrl+Shift+P → `Dev Containers: Reopen in Container`
3. `cargo build --release`

---

## 🔨 Compilation & Tests

### Compilation

```bash
cargo build              # debug
cargo build --release    # optimisé
```

Le `build.rs` compile automatiquement librtlsdr via CMake et génère les bindings FFI via bindgen.

### Tests unitaires

```bash
cargo test               # tous les tests
cargo test --lib         # tests unitaires uniquement
cargo test viterbi       # filtrer par nom
```

Les tests sont organisés en modules `#[cfg(test)]` inline, mirrorant la structure `src/` :

```
src/
  dab_constants.rs          → tests CRC, bit extraction, constantes
  support/
    dab_params.rs           → tests modes DAB I/II/III/IV
    band_handler.rs         → tests fréquences canaux
    ringbuffer.rs           → tests buffer thread-safe
  ofdm/
    freq_interleaver.rs     → tests permutation carriers
    phase_table.rs          → tests table de phase
  eti_handling/
    viterbi_handler.rs      → tests décodeur Viterbi
    fic_handler.rs          → tests traitement FIC
    cif_interleaver.rs      → tests entrelacement CIF
    protection.rs           → tests EEP/UEP
    prot_tables.rs          → tests tables de poinçonnage
  eti2pcm/
    crc.rs                  → tests CRC-16-CCITT, Fire Code
    eti_reader.rs           → tests lecture ETI (sync FSYNC)
    eti_frame.rs            → tests parsing trame ETI
    fic_decoder.rs          → tests FIG 0/0, 0/1, 0/2, 1/0, 1/1
    rs_decoder.rs           → tests Reed-Solomon GF(2^8)
    superframe.rs           → tests superframe DAB+
    pad_decoder.rs          → tests PAD / DLS / MOT slideshow
    pad_output.rs           → tests JSON fd 3 + slideshow
    mot_decoder.rs          → tests X-PAD MOT DataGroup decoder
    mot_manager.rs          → tests MOT object reassembly
```

---

## 🔀 Cross-Compilation

### ARM64 (Raspberry Pi 3/4 64-bit)

```bash
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
```

### ARM32 (Raspberry Pi 2/3 32-bit)

```bash
rustup target add armv7-unknown-linux-gnueabihf
sudo apt install -y gcc-arm-linux-gnueabihf
cargo build --release --target armv7-unknown-linux-gnueabihf
```

### Windows (via cargo-xwin)

```bash
rustup target add x86_64-pc-windows-msvc
cargo xwin build --release --target x86_64-pc-windows-msvc
```

### Configuration `.cargo/config.toml`

```toml
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

### Déploiement

```bash
scp target/aarch64-unknown-linux-gnu/release/eti-rtlsdr-rust user@rpi:/usr/local/bin/
```

### Packaging release (.tar.gz)

#### AMD64

```bash
cargo build --release
VERSION=$(cargo metadata --no-deps --format-version 1 | grep -o '"version":"[^"]*"' | head -1 | cut -d'"' -f4)
LIB_DIR=$(find target/release/build -path '*/out/lib/librtlsdr.so.0' | head -1 | xargs dirname)
tar -czf eti-rtlsdr-rust-${VERSION}-x86_64-linux.tar.gz \
  -C target/release eti-rtlsdr-rust \
  -C "$(pwd)/${LIB_DIR}" librtlsdr.so.0
```

#### ARM64

```bash
cargo build --release --target aarch64-unknown-linux-gnu
VERSION=$(cargo metadata --no-deps --format-version 1 | grep -o '"version":"[^"]*"' | head -1 | cut -d'"' -f4)
LIB_DIR=$(find target/aarch64-unknown-linux-gnu/release/build -path '*/out/lib/librtlsdr.so.0' | head -1 | xargs dirname)
tar -czf eti-rtlsdr-rust-${VERSION}-aarch64-linux.tar.gz \
  -C target/aarch64-unknown-linux-gnu/release eti-rtlsdr-rust \
  -C "$(pwd)/${LIB_DIR}" librtlsdr.so.0
```

> Les archives contiennent le binaire et `librtlsdr.so.0`. Sur la cible, installer `libusb-1.0-0` puis :
> ```bash
> tar xzf eti-rtlsdr-rust-*-linux.tar.gz
> sudo cp eti-rtlsdr-rust /usr/local/bin/
> sudo cp librtlsdr.so.0 /usr/local/lib/
> sudo ldconfig
> ```

---

## ⚙️ Options CLI

Le binaire expose deux sous-commandes :

```
eti-rtlsdr-rust <COMMAND>
  iq2eti     Générer un flux ETI depuis RTL-SDR (IQ → ETI)
  eti2pcm    Décoder un flux ETI en audio PCM (comme dablin)
```

### `iq2eti` — RTL-SDR → ETI

```
eti-rtlsdr-rust iq2eti [OPTIONS]
```

| Option | Court | Description | Défaut |
|---|---|---|---|
| `--channel` | `-C` | Canal DAB (5A, 6C, 11C, 12C…) | `11C` |
| `--gain` | `-G` | Gain en % (0–100) | `50` |
| `--ppm` | `-p` | Correction fréquence (PPM) | `0` |
| `--autogain` | `-Q` | AGC automatique | off |
| `--sync-time` | `-d` | Timeout sync temps (sec) | `5` |
| `--detect-time` | `-D` | Timeout détection ensemble (sec) | `10` |
| `--output` | `-O` | Fichier de sortie (`-` = stdout) | stdout |
| `--record-time` | `-t` | Durée enregistrement sec (-1 = ∞) | `-1` |
| `--silent` | `-S` | Mode silencieux (pas de log stderr) | off |
| `--device-index` | | Index dongle RTL-SDR | `0` |

### `eti2pcm` — ETI → PCM audio

```
eti-rtlsdr-rust eti2pcm [OPTIONS] [FILE]
```

| Option | Court | Description | Défaut |
|---|---|---|---|
| `--sid` | `-s` | Service ID hex (ex: `0xF2F8`) | — |
| `--label` | `-l` | Sélection par nom de service | — |
| `--first` | `-1` | Jouer le premier service trouvé | off |
| `--pcm` | `-p` | Sortie PCM 16-bit sur stdout | off |
| `--disable-dyn-fic` | `-F` | Désactiver les messages FIC sur stderr | off |
| `--slide-dir` | `-S` | Sauvegarder les images slideshow dans ce dossier | — |
| `--slide-base64` | | Sortir les slides en base64 JSON sur fd 3 | off |
| `[FILE]` | | Fichier ETI (défaut : stdin) | stdin |

**Sortie audio :** PCM signé 16-bit little-endian, stéréo, 48 kHz (ou 32 kHz pour certains services DAB+).

**Métadonnées JSON (fd 3) :** Si le file descriptor 3 est ouvert, `eti2pcm` y écrit les métadonnées DAB au format JSON, une ligne par événement :
- `{"ensemble":{"eid":"0x...","label":"..."}}` — informations ensemble
- `{"service":{"sid":"0x...","label":"..."}}` — service sélectionné
- `{"dl":"..."}` — Dynamic Label (texte en cours de diffusion)
- `{"slide":{"contentName":"...","contentType":"image/jpeg","data":"base64..."}}` — slideshow (avec `--slide-base64`)

**Slideshow :** Les images MOT (JPEG/PNG) diffusées via X-PAD peuvent être :
- Sauvegardées sur disque avec `-S /chemin/dossier`
- Envoyées en base64 JSON sur fd 3 avec `--slide-base64`

### Canaux bande III

5A–13F (174.928–239.200 MHz). Les canaux L-Band (LA–LP, 1452–1478 MHz) sont aussi supportés.

---

## 💡 Exemples

### Pipeline complète : RTL-SDR → PCM → lecteur audio

```bash
# Écouter NRJ (SID 0xF2F8) sur le canal 6C sans dépendance externe (sauf ffplay)
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 \
  | ./target/release/eti-rtlsdr-rust eti2pcm -F -s 0xF2F8 -p \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -
```

### Pipeline avec métadonnées JSON

```bash
# fd 3 redirigé vers un fichier pour capturer DLS, ensemble, service
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 \
  | ./target/release/eti-rtlsdr-rust eti2pcm -F -s 0xF2F8 -p 3>metadata.json \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -
```

### Décoder un fichier ETI existant

```bash
# Décoder un fichier ETI capturé en PCM
./target/release/eti-rtlsdr-rust eti2pcm -s 0xF2F8 -p capture.eti > output.raw
ffplay -f s16le -ar 48000 -ac 2 output.raw

# Ou par nom de service
./target/release/eti-rtlsdr-rust eti2pcm -l "NRJ" -p capture.eti > output.raw
```

### Jouer le premier service trouvé

```bash
./target/release/eti-rtlsdr-rust eti2pcm -1 -p < capture.eti | aplay -f S16_LE -r 48000 -c 2
```

### Recevoir et sauvegarder un fichier ETI

```bash
sudo ./target/release/eti-rtlsdr-rust iq2eti -C 6C -G 20 -O "6C_$(date +%F_%H%M).eti"
```

### Pipeline vers dablin (compatibilité)

```bash
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 | dablin_gtk -L
```

### dablin CLI avec sélection de service

```bash
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 | dablin -F -s 0xF2F8 -p
```

### Enregistrement limité à 60 secondes

```bash
sudo ./target/release/eti-rtlsdr-rust iq2eti -C 6C -G 20 -t 60 -O capture.eti
```

### Convertir en WAV (via eti2pcm + ffmpeg)

```bash
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 -t 15 \
  | ./target/release/eti-rtlsdr-rust eti2pcm -s 0xF2F8 -p \
  | ffmpeg -f s16le -ar 48000 -ac 2 -i - output.wav
```

### Sauvegarder les images slideshow

```bash
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 \
  | ./target/release/eti-rtlsdr-rust eti2pcm -s 0xF2F8 -p -S /tmp/slides \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -
# Les images JPEG/PNG reçues sont sauvegardées dans /tmp/slides/
```

### Slideshow en base64 JSON (fd 3)

```bash
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 \
  | ./target/release/eti-rtlsdr-rust eti2pcm -s 0xF2F8 -p --slide-base64 3>pad.json \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -
# Le fichier pad.json contient les infos DAB + les slides en base64
```

### Test fonctionnel automatisé

```bash
sudo bash test-capture.sh 6C 20
```

Vérifie : build → détection dongle → capture 10s → validation frames ETI.

---

## 📚 Tutoriel : de zéro à l'écoute DAB

### Étape 1 — Installation

```bash
# Cloner et builder
git clone https://github.com/votre-user/eti-rtlsdr-rust.git
cd eti-rtlsdr-rust
cargo build --release
```

### Étape 2 — Brancher le dongle RTL-SDR

```bash
# Vérifier la détection USB
lsusb | grep -i rtl
# → Bus 001 Device 003: ID 0bda:2838 Realtek RTL2838UHIDIR

# Tester le dongle
rtl_test -t
# → Found 1 device(s): ...
```

> **Note** : Si `rtl_test` échoue, blacklister le driver DVB kernel :
> ```bash
> sudo rmmod dvb_usb_rtl28xxu 2>/dev/null
> echo "blacklist dvb_usb_rtl28xxu" | sudo tee /etc/modprobe.d/rtlsdr.conf
> ```

### Étape 3 — Scanner les canaux

Le DAB en France utilise la bande III. Les multiplexes principaux :

| Canal | Fréquence | Contenu typique |
|---|---|---|
| 5A | 174.928 MHz | Régional |
| 6A–6D | 181–185 MHz | Métropolitain |
| 11C | 220.352 MHz | Métropolitain |
| 12C | 227.360 MHz | National |

```bash
# Tester un canal (ex: 6C à Paris)
./eti-rtlsdr-rust.sh iq2eti -C 6C -G 20 -t 5 -O /dev/null
```

Si vous voyez `ensemble ... detected` et des `program ... is in the list`, le canal fonctionne.

### Étape 4 — Capturer un fichier ETI

```bash
# Capturer 60 secondes du canal 6C
./eti-rtlsdr-rust.sh iq2eti -C 6C -G 20 -t 60 -O capture_6C.eti
```

Le fichier ETI peut être relu plus tard avec `eti2pcm` sans le dongle.

### Étape 5 — Écouter en direct (pipeline intégrée)

```bash
# Écouter un programme spécifique (ex: NRJ, SID 0xF2F8)
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 \
  | ./target/release/eti-rtlsdr-rust eti2pcm -F -s 0xF2F8 -p \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# Ou avec dablin (si installé)
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 | dablin -s 0xF2F8

# Ou avec l'interface graphique dablin
sudo ./target/release/eti-rtlsdr-rust iq2eti -S -C 6C -G 20 | dablin_gtk
```

> **Astuce** : lancez d'abord sans `-S` pour voir les SID des programmes disponibles dans stderr, puis relancez avec `-S` et `-s 0xSID`.

### Étape 6 — Relire une capture

```bash
# Relire le fichier ETI capturé avec eti2pcm (intégré)
./target/release/eti-rtlsdr-rust eti2pcm -s 0xF2F8 -p capture_6C.eti \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# Ou avec dablin (si installé)
dablin -s 0xF2F8 < capture_6C.eti
dablin_gtk < capture_6C.eti
```

### Étape 7 — Raspberry Pi

```bash
# Cross-compiler pour Pi 4 (64-bit)
cargo build --release --target aarch64-unknown-linux-gnu

# Déployer
scp target/aarch64-unknown-linux-gnu/release/eti-rtlsdr-rust pi@raspberrypi:~

# Sur le Pi — écouter en direct
ssh pi@raspberrypi
./eti-rtlsdr-rust iq2eti -S -C 6C -G 30 \
  | ./eti-rtlsdr-rust eti2pcm -F -s 0xF2F8 -p \
  | aplay -f S16_LE -r 48000 -c 2
```

---

## 🏗️ Architecture

```
build.rs              CMake librtlsdr + bindgen FFI + link faad/mpg123
src/
  main.rs             CLI (clap sous-commandes) → routage iq2eti / eti2pcm
  lib.rs              Déclarations modules
  iq2eti.rs           Sous-commande iq2eti (ancien main.rs)
  eti2pcm_cmd.rs      Sous-commande eti2pcm (pipeline ETI → PCM)
  rtlsdr_sys.rs       FFI bindings auto-générés
  dab_constants.rs    Constantes, CRC, bit utils
  support/
    dab_params.rs     Paramètres DAB Modes I–IV
    band_handler.rs   Canal → fréquence
    ringbuffer.rs     Buffer IQ thread-safe
  ofdm/
    phase_table.rs    Table de phase Mode I
    phase_reference.rs  Corrélation sync + CFO
    freq_interleaver.rs Permutation carriers
    ofdm_processor.rs   Boucle OFDM principale
  eti_handling/
    prot_tables.rs    24 tables de poinçonnage
    viterbi_handler.rs  Décodeur Viterbi {0155,0117,0123,0155}
    fic_handler.rs    Dépoinçonnage/décodage FIC
    fib_processor.rs  Parsing FIG0/0, FIG0/1, FIG1
    protection.rs     Déconvolution EEP + UEP
    cif_interleaver.rs  Entrelacement CIF 16 trames
    eti_generator.rs  Construction trame ETI
  device/
    rtlsdr_handler.rs RTL-SDR via FFI C
  eti2pcm/
    crc.rs            CRC-16-CCITT + Fire Code
    eti_reader.rs     Lecture trames ETI 6144 octets (sync FSYNC)
    eti_frame.rs      Parsing en-tête ETI (ERR, FC, STC, EOH)
    fic_decoder.rs    Décodage FIC/FIG pour découverte services
    rs_decoder.rs     Reed-Solomon (120,110) GF(2^8) pur Rust
    superframe.rs     Superframe DAB+ (5 frames → RS → AU)
    aac_decoder.rs    FFI libfaad2 (décodage AAC DAB+)
    mp2_decoder.rs    FFI libmpg123 (décodage MP2 DAB)
    pad_decoder.rs    PAD : F-PAD + X-PAD + DLS (Dynamic Label) + MOT slideshow
    pad_output.rs     Sortie JSON métadonnées + slideshow sur fd 3
    mot_decoder.rs    X-PAD → MOT DataGroup (accumulation + CRC)
    mot_manager.rs    MOT DataGroup → objet MOT (header+body → image JPEG/PNG)
```

### Threads (iq2eti)

1. **Main** : CLI, sync detection, status display
2. **OFDM** : `ofdm_processor.run()` lit IQ depuis device, démodule, envoie blocs
3. **ETI** : `eti_generator.run_loop()` reçoit blocs, FIC + CIF interleaving + sortie ETI

### Pipeline (eti2pcm)

```
stdin/fichier ETI
  → EtiReader (sync FSYNC, blocs 6144 octets)
    → parse_eti_frame (en-tête, FIC, sous-canaux)
      → FicDecoder (FIG 0/0, 0/1, 0/2, 1/0, 1/1 → découverte services)
      → subchannel_data(subchid)
        → SuperframeFilter (5 frames → RS(120,110) → AU)
          → AacDecoder (libfaad2) ou Mp2Decoder (libmpg123)
            → PCM 16-bit stdout
        → PadDecoder (F-PAD + X-PAD → DLS)
          → PadOutput (JSON fd 3)
```

---

## 🩺 Dépannage

### `librtlsdr.so.0 not found`

```bash
export LD_LIBRARY_PATH=$(find target -name "librtlsdr.so.0" 2>/dev/null | head -1 | xargs dirname):$LD_LIBRARY_PATH
```

### `No RTL-SDR devices found`

- Vérifier connexion USB : `lsusb | grep 0bda`
- Blacklister le driver kernel DVB : `echo "blacklist dvb_usb_rtl28xxu" | sudo tee /etc/modprobe.d/rtlsdr.conf`
- Recharger : `sudo rmmod dvb_usb_rtl28xxu 2>/dev/null`

### Pas de sync / signal faible

- Augmenter le gain : `-G 90`
- Essayer autogain : `-Q`
- Vérifier l'antenne et sa polarisation

---

## 🚀 Optimisation des performances

### Fait (v0.2)

- **Viterbi sans allocation** : élimination des `.clone()` de métriques (double-buffering par destructuration)
- **Profil release optimisé** : `lto = true`, `codegen-units = 1`
- **Pré-allocation des buffers OFDM** : `null_buf`, `check_buf`, `block_buf` alloués une seule fois avant la boucle principale
- **Inline de `process_block_0`** : évite le `.to_vec()` par symbole
- **Phase table O(1)** : lookup direct par calcul d’index `(k+768)/32` au lieu d’un scan linéaire de 48 entrées
- **Pré-allocation `process_cif`** : buffer `out_vector` alloué une seule fois (9216 octets max) et réutilisé pour tous les sous-canaux
- **Bit packing extrait** : fonction `pack_bits` dédiée
- **Batch IQ read** : lectures IQ par blocs (`get_samples`) au lieu de sample-par-sample pour l’init, le frame skip et la synchronisation de phase

### Fait (v0.3)

- **NCO oscillateur** : remplacement de la table 2M entrées (16 MB) par un NCO incrémental à 2 floats (`nco_phase += delta`)
- **Zero-copy channel** : remplacement de `mpsc::sync_channel<BufferElement>` par un ring buffer SPSC pré-alloué (512 slots fixes, zéro allocation par frame)
- **Viterbi branchless** : réécriture de `update_viterbi()` avec sélection branchless (`XOR + mask`) pour auto-vectorisation LLVM
- **Parallel subchannel** : déconvolution Viterbi des sous-canaux en parallèle via `rayon::par_iter` dans `process_cif()`

### Fait (v0.4)

- **Logging structuré** : migration complète `eprintln!` → `tracing` (info/warn/error) avec filtrage par niveau (`-S` = off)
- **EBU Latin → UTF-8** : conversion charset EN 300 401 pour les noms d'ensembles et services DAB
- **FIC quality callback** : remontée en temps réel de la qualité FIB (CRC) depuis le thread ETI vers l'affichage
- **Zero-alloc OFDM sync** : remplacement de `.to_vec()` (16 KB/frame) par `copy_within()` dans la boucle de synchronisation
- **FIB buffer hoisting** : `fibs_bytes` (3 KB) sorti de la boucle ETI, réutilisé via `.fill(0)`
- **Thread-local subchannel buffers** : `out_vector` par job rayon via `thread_local!` (évite ~384 KB d'allocs par CIF)
- **Strip + panic=abort** : binaire release strippé (2.4 MB), `panic = "abort"` élimine le code d'unwinding

### Axes restants (roadmap)

| Axe | Description | Impact estimé |
|---|---|---|
| **SIMD Viterbi natif** | Utiliser `std::simd` (nightly) ou intrinsics x86/ARM pour traiter 4/8 états SIMD | ~2x suppl. |
| **FFT scratch réutilisation** | Passer le scratch buffer FFT explicitement pour éviter les allocations internes rustfft | Réduction alloc FFT |
| **Lock-free FIB pipeline** | Découpler le FIC processing du thread ETI principal | Réduction latence |

---

## 📖 Man page

Une page de manuel Unix est fournie :

```bash
# Consulter localement
man ./eti-rtlsdr-rust.1

# Installer system-wide
sudo install -m 644 eti-rtlsdr-rust.1 /usr/local/share/man/man1/
sudo mandb
man eti-rtlsdr-rust
```

---

## 📄 Licence

GPL-2.0 — Même licence que librtlsdr.

## 🔗 Références

- [eti-stuff](https://github.com/JvanKatwijk/eti-stuff) — Implémentation C++ de référence (IQ → ETI)
- [dablin](https://github.com/Opendigitalradio/dablin) — Décodeur ETI → audio (C++, base du port `eti2pcm`)
- [rtl-sdr](https://github.com/osmocom/rtl-sdr) — Driver RTL-SDR
- [ETSI EN 300 401](https://www.etsi.org/deliver/etsi_en/300400_300499/300401/) — Norme DAB
- [ETSI TS 102 563](https://www.etsi.org/deliver/etsi_ts/102500_102599/102563/) — Norme DAB+
