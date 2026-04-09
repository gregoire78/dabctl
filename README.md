<div align="center">

# 📡 dabctl

**Réception DAB complète en Rust : RTL-SDR → PCM audio**

Port Rust de [eti-cmdline](https://github.com/JvanKatwijk/eti-stuff) (IQ → ETI) et [dablin](https://github.com/Opendigitalradio/dablin) (ETI → audio).
Pipeline directe **RTL-SDR → PCM** en un seul processus.

[![Rust](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org/)
[![License: GPL-2.0](https://img.shields.io/badge/License-GPL%202.0-blue.svg)](COPYING)

</div>

---

## ⚡ Démarrage rapide

```bash
# 1. Build
cargo build --release

# 2. Écouter NRJ (SID 0xF2F8) sur le canal 6C
sudo ./target/release/dabctl -C 6C -s 0xF2F8 \
  | ffplay -f s16le -ar 48000 -ac 2 -i -

# 3. Avec gain manuel (20%)
sudo ./target/release/dabctl -C 6C -s 0xF2F8 -G 20 \
  | ffplay -f s16le -ar 48000 -ac 2 -i -
```

---

## 📋 Prérequis

### Système

| Paquet | Rôle |
|---|---|
| `libusb-1.0-0-dev` | USB backend pour RTL-SDR (requis par `rtl-sdr-rs`) |
| `pkg-config` | Découverte de libs système |
| `build-essential` | Compilateur C (requis par libfaad2/libmpg123) |
| `libfaad-dev` | Décodeur AAC pour DAB+ (backend par défaut) |
| `libfdk-aac-dev` | Décodeur AAC alternatif — Fraunhofer FDK (optionnel, feature `fdk-aac`) |
| `libmpg123-dev` | Décodeur MP2 pour DAB classique |

```bash
# Backend faad2 (défaut)
sudo apt install -y libusb-1.0-0-dev pkg-config build-essential libfaad-dev libmpg123-dev

# Backend fdk-aac (optionnel) — requiert le composant non-free de Debian/Ubuntu
# Sur Debian Trixie :
sudo sed -i 's/Components: main$/Components: main non-free/' /etc/apt/sources.list.d/debian.sources
sudo apt update
sudo apt install -y libusb-1.0-0-dev pkg-config build-essential libfdk-aac-dev libmpg123-dev
# Sur Ubuntu :
# sudo add-apt-repository universe && sudo apt install -y libfdk-aac-dev
```

> **Note :** `libfdk-aac-dev` est dans le composant `non-free` de Debian (brevets audio Fraunhofer).
> Sur les systèmes où `non-free` n'est pas disponible, utilisez le backend par défaut **faad2**.

### Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Bibliothèques liées

| Bibliothèque | Version | Rôle | Lien |
|---|---|---|---|
| **libusb-1.0** | 1.0+ | Backend USB pour RTL-SDR (via `rtl-sdr-rs`) | [libusb.info](https://libusb.info) |
| **libfaad2** | 2.11+ | Décodeur AAC DAB+ — backend par défaut | [knik-o/faad2](https://github.com/knik-o/faad2) |
| **libfdk-aac** | 2.0+ | Décodeur AAC DAB+ — backend optionnel (feature `fdk-aac`) | [Fraunhofer FDK AAC](https://github.com/mstorsjo/fdk-aac) |
| **libmpg123** | 1.32+ | Décodeur MP2 (DAB classique) pour `eti2pcm` | [mpg123.de](https://mpg123.de) |

> **Note :** Le pilote RTL-SDR est géré par la crate Rust [`rtl-sdr-rs`](https://github.com/ccostes/rtl-sdr-rs), un port pur Rust de la bibliothèque Osmocom. Aucun CMake, `bindgen` ni `libclang` n'est requis. Seule `libusb-1.0` doit être installée sur le système.

### Crates Rust

| Crate | Version | Rôle |
|---|---|---|
| `clap` | 4.4 | Parsing des arguments CLI |
| `rustfft` | 6.4 | FFT pour démodulation OFDM |
| `num-complex` | 0.4 | Types complexes (IQ) |
| `rayon` | 1.10 | Parallélisation des sous-canaux |
| `tracing` | 0.1 | Logging structuré |
| `tracing-subscriber` | 0.3 | Formatage et filtrage des logs |
| `anyhow` | 1.0 | Gestion d'erreurs |
| `ctrlc` | 3.4 | Handler Ctrl-C |
| `libc` | 0.2 | Écriture JSON sur fd 3 |
| `serde` | 1.0 | Sérialisation |
| `serde_json` | 1.0 | Sortie JSON métadonnées DAB |
| `base64` | 0.22 | Encodage base64 des images slideshow |
| `rtl-sdr-rs` | 0.3 | Pilote RTL-SDR pur Rust (via `rusb`) |
| `encoding_rs` | 0.8 | Décodage EBU Latin/UTF-8 pour DLS |

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

### Décodeur AAC : faad2 (défaut) vs fdk-aac (optionnel)

La sélection du backend AAC est inspirée de l'option `USE_FDKAAC` de [AbracaDABra](https://github.com/KejPi/AbracaDABra) (KejPi, MIT licence), adaptée en Cargo feature.

| Feature | Backend | Librairie système |
|---|---|---|
| *(aucune)* | faad2 (défaut) | `libfaad-dev` |
| `fdk-aac` | Fraunhofer FDK AAC | `libfdk-aac-dev` |

```bash
# faad2 (défaut) — aucune option requise
cargo build --release

# fdk-aac (optionnel)
cargo build --release --features fdk-aac
```

### Compilation

```bash
cargo build              # debug
cargo build --release    # optimisé
```

Le `build.rs` ne contient que les directives de linkage pour le décodeur AAC et `libmpg123`. Le pilote RTL-SDR est intégralement géré par la crate `rtl-sdr-rs` via `rusb` (pas de CMake, pas de C compilé).

### Tests unitaires

```bash
cargo test               # tous les tests
cargo test --lib         # tests unitaires uniquement
cargo test viterbi       # filtrer par nom
```

Les tests sont organisés en modules `#[cfg(test)]` inline, mirrorant la structure `src/` :

```
src/
  pipeline/
    dab_constants.rs          → tests CRC, bit extraction, constantes
    dab_params.rs             → tests modes DAB I/II/III/IV
    band_handler.rs           → tests fréquences canaux
    ringbuffer.rs             → tests buffer thread-safe
    ofdm/
      freq_interleaver.rs     → tests permutation carriers
      phase_table.rs          → tests table de phase
    viterbi_handler.rs        → tests décodeur Viterbi
    fic_handler.rs            → tests traitement FIC
    cif_interleaver.rs        → tests entrelacement CIF
    protection.rs             → tests EEP/UEP
    prot_tables.rs            → tests tables de poinçonnage
  audio/
    crc.rs                    → tests CRC-16-CCITT, Fire Code
    fic_decoder.rs            → tests FIG 0/0, 0/1, 0/2, 1/0, 1/1
    rs_decoder.rs             → tests Reed-Solomon GF(2^8)
    superframe.rs             → tests superframe DAB+
    pad_decoder.rs            → tests PAD / DLS / MOT slideshow
    pad_output.rs             → tests JSON fd 3 + slideshow
    mot_decoder.rs            → tests X-PAD MOT DataGroup decoder
    mot_manager.rs            → tests MOT object reassembly
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
scp target/aarch64-unknown-linux-gnu/release/dabctl user@rpi:/usr/local/bin/
```

### Packaging release (.tar.gz)

#### AMD64

```bash
cargo build --release
VERSION=$(cargo metadata --no-deps --format-version 1 | grep -o '"version":"[^"]*"' | head -1 | cut -d'"' -f4)
tar -czf dabctl-${VERSION}-x86_64-linux.tar.gz \
  -C target/release dabctl
```

#### ARM64

```bash
cargo build --release --target aarch64-unknown-linux-gnu
VERSION=$(cargo metadata --no-deps --format-version 1 | grep -o '"version":"[^"]*"' | head -1 | cut -d'"' -f4)
tar -czf dabctl-${VERSION}-aarch64-linux.tar.gz \
  -C target/aarch64-unknown-linux-gnu/release dabctl
```

> Les archives contiennent le binaire statiquement lié à `rtl-sdr-rs`.
> Sur la cible, installer `libusb-1.0-0`, `libfaad2` et `libmpg123` puis :
> ```bash
> tar xzf dabctl-*-linux.tar.gz
> sudo cp dabctl /usr/local/bin/
> ```

---

## ⚙️ Options CLI

```
dabctl [OPTIONS]
```

| Option | Court | Description | Défaut |
|---|---|---|---|
| `--channel` | `-C` | Canal DAB (5A, 6C, 11C, 12C…) | **requis** |
| `--sid` | `-s` | Service ID hex (ex: `0xF2F8`) | **requis** |
| `--gain` | `-G` | Gain en % (0–100) | auto-gain |
| `--ppm` | `-p` | Correction fréquence (PPM) | `0` |
| `--sync-time` | `-d` | Timeout sync temps (sec) | `5` |
| `--detect-time` | `-D` | Timeout détection ensemble (sec) | `10` |
| `--label` | `-l` | Sélection par nom de service | — |
| `--disable-dyn-fic` | `-F` | Désactiver les messages FIC sur stderr | off |
| `--slide-dir` | `-S` | Sauvegarder les images slideshow dans ce dossier | — |
| `--slide-base64` | | Sortir les slides en base64 JSON sur fd 3 | off |
| `--silent` | | Mode silencieux (pas de log stderr) | off |
| `--device-index` | | Index dongle RTL-SDR | `0` |
| `--aac-decoder` | | Backend AAC : `faad2` ou `fdk-aac` (feature `fdk-aac` requise) | `fdk-aac` |

**Sortie audio :** PCM signé 16-bit little-endian, stéréo, 48 kHz sur stdout.

**Métadonnées JSON (fd 3 ou `-m`) :** une ligne par événement :
- `{"ensemble":{"eid":"0x...","label":"..."}}` — informations ensemble
- `{"service":{"sid":"0x...","label":"..."}}` — service sélectionné
- `{"dl":"..."}` — Dynamic Label (texte en cours de diffusion)
- `{"slide":{"contentName":"...","contentType":"image/jpeg","data":"base64..."}}` — slideshow (avec `--slide-base64`)

### Canaux bande III

5A–13F (174.928–239.200 MHz). Les canaux L-Band (LA–LP, 1452–1478 MHz) sont aussi supportés.

---

## 💡 Exemples

### Écouter en direct

```bash
# Écouter NRJ (SID 0xF2F8) sur le canal 6C (auto-gain)
sudo ./target/release/dabctl -C 6C -s 0xF2F8 \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# Avec gain manuel (20%) et correction PPM
sudo ./target/release/dabctl -C 6C -s 0xF2F8 -G 20 -p 2 \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# Avec aplay au lieu de ffplay
sudo ./target/release/dabctl -C 11C -s 0xF2F8 -G 50 \
  | aplay -f S16_LE -r 48000 -c 2
```

### Avec métadonnées JSON et slideshow

```bash
# Métadonnées JSON sur fd 3
sudo ./target/release/dabctl -C 6C -s 0xF2F8 \
  --slide-base64 3>pad_metadata.json \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -

# Avec slides sur disque + JSON sur fd 3
sudo ./target/release/dabctl -C 6C -s 0xF2F8 \
  --slide-dir /tmp/slides --slide-base64 3>pad_metadata.json \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -
```

### Convertir en WAV

```bash
sudo ./target/release/dabctl -C 6C -s 0xF2F8 -G 20 \
  | sox -t raw -r 48000 -b 16 -c 2 -e signed-integer -L - output.wav
```

### Capture automatisée

```bash
# Script prêt à l'emploi
./live-capture-iq2pcm.sh 6C 0xF2F8 20
# → audio (output.wav) + métadonnées (pad_metadata.json) + slides (slides/)
```

---

## 📚 Tutoriel : de zéro à l'écoute DAB

### Étape 1 — Installation

```bash
# Cloner et builder
git clone https://github.com/votre-user/dabctl.git
cd dabctl
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
sudo ./target/release/dabctl -C 6C -s 0xF2F8 --silent 2>/dev/null | head -c 0
```

Si vous voyez `ensemble ... detected` et des `program ... is in the list` dans les logs (sans `--silent`), le canal fonctionne.

### Étape 4 — Écouter en direct

```bash
# Écouter un programme spécifique (ex: NRJ, SID 0xF2F8)
sudo ./target/release/dabctl -C 6C -s 0xF2F8 \
  | ffplay -f s16le -ar 48000 -ac 2 -nodisp -i -
```

> **Astuce** : lancez d'abord sans `--silent` pour voir les SID des programmes disponibles dans stderr.

### Étape 5 — Raspberry Pi

```bash
# Cross-compiler pour Pi 4 (64-bit)
cargo build --release --target aarch64-unknown-linux-gnu

# Déployer
scp target/aarch64-unknown-linux-gnu/release/dabctl pi@raspberrypi:~

# Sur le Pi — écouter en direct
ssh pi@raspberrypi
./dabctl -C 6C -s 0xF2F8 -G 30 \
  | aplay -f S16_LE -r 48000 -c 2
```

---

## 🏗️ Architecture

```
build.rs              Directives de linkage faad/mpg123
src/
  main.rs             CLI (clap) → pipeline RTL-SDR → PCM
  lib.rs              Déclarations modules
  iq2pcm_cmd.rs       Pipeline principale : RTL-SDR → PCM
  pipeline/
    dab_constants.rs    Constantes, CRC, bit utils
    dab_frame.rs        Type DabFrame (transport in-process : FIC + sous-canaux)
    dab_params.rs       Paramètres DAB Modes I–IV
    band_handler.rs     Canal → fréquence
    ringbuffer.rs       Buffer IQ thread-safe
    subchannel_pool.rs  Pool de buffers réutilisables (zéro alloc/frame)
    eti_generator.rs    DabPipeline : OFDM blocs → DabFrame (canal mpsc)
    fib_processor.rs    Parsing FIG0/0, FIG0/1, FIG1
    fic_handler.rs      Dépoinçonnage/décodage FIC
    viterbi_handler.rs  Décodeur Viterbi {0155,0117,0123,0155}
    prot_tables.rs      24 tables de poinçonnage
    protection.rs       Déconvolution EEP + UEP
    cif_interleaver.rs  Entrelacement CIF 16 trames
    ofdm/
      phase_table.rs       Table de phase Mode I
      phase_reference.rs   Corrélation sync + CFO
      freq_interleaver.rs  Permutation carriers
      ofdm_processor.rs    Boucle OFDM principale
  device/
    rtlsdr_handler.rs    RTL-SDR via rtl-sdr-rs
  audio/
    crc.rs               CRC-16-CCITT + Fire Code
    fic_decoder.rs       Décodage FIC/FIG pour découverte services
    rs_decoder.rs        Reed-Solomon (120,110) GF(2^8) pur Rust
    superframe.rs        Superframe DAB+ (5 frames → RS → AU)
    aac_decoder.rs       FFI libfaad2 (décodage AAC DAB+)
    mp2_decoder.rs       FFI libmpg123 (décodage MP2 DAB)
    pad_decoder.rs       PAD : F-PAD + X-PAD + DLS (Dynamic Label) + MOT slideshow
    pad_output.rs        Sortie JSON métadonnées + slideshow sur fd 3
    mot_decoder.rs       X-PAD → MOT DataGroup (accumulation + CRC)
    mot_manager.rs       MOT DataGroup → objet MOT (header+body → image JPEG/PNG)
    ebu_latin.rs         EBU Latin → UTF-8 conversion
```

### Threads

1. **OFDM** : `ofdm_processor.run()` lit IQ depuis le dongle, démodule, envoie des blocs à `DabPipeline`
2. **DabPipeline** (thread interne) : reçoit les blocs OFDM via ring SPSC, effectue le FIC + CIF interleaving + déconvolution Viterbi des sous-canaux, émet un `DabFrame` par CIF via un canal `mpsc` à capacité bornée (4 frames ≈ 100 ms de back-pressure)
3. **Audio/main** : consomme les `DabFrame` du canal, décode FIC, alimente `SuperframeFilter` → `AacDecoder`/`Mp2Decoder`, `PadDecoder` → sortie PCM stdout + JSON fd 3

### Pipeline

```
RTL-SDR (2.048 MHz IQ)
  → RtlsdrHandler (USB → f32)
    → OfdmProcessor (FFT, sync, phase)
      → DabPipeline (FIC + CIF → DabFrame)
        → FicDecoder (découverte services)
        → SuperframeFilter (5 frames → RS → AU)
          → AacDecoder (DAB+) ou Mp2Decoder (DAB)
            → PCM 16-bit stdout
        → PadDecoder (DLS + MOT)
          → PadOutput (JSON fd 3)
```

---

## 🩺 Dépannage

### `No RTL-SDR devices found`

- Vérifier connexion USB : `lsusb | grep 0bda`
- Blacklister le driver kernel DVB : `echo "blacklist dvb_usb_rtl28xxu" | sudo tee /etc/modprobe.d/rtlsdr.conf`
- Recharger : `sudo rmmod dvb_usb_rtl28xxu 2>/dev/null`

### Pas de sync / signal faible

- Augmenter le gain : `-G 90`
- Essayer auto-gain : ne pas passer `-G`
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

### Fait (v0.5)

- **`iq2pcm` : chemin direct RTL-SDR → PCM** : nouveau transport `DabFrame` in-process — la sérialisation ETI 6144 octets est sautée sur le chemin `iq2pcm`, réduisant la copie de données et la latence de bout en bout
- **`DabFrame` zero-copy** : les payloads sous-canaux sont partagés via `Arc<[u8]>` entre `DabPipeline`, `EtiSerializer` et le décodeur audio — aucune copie supplémentaire
- **`SmallVec<[SubchannelFrame; 8]>`** : stockage inline pour jusqu'à 8 sous-canaux par CIF (multiplex DAB typique), zéro allocation heap sur le chemin critique
- **`SubchannelPool`** : pool de buffers pré-alloués pour les payloads sous-canaux — élimine les allocations `Vec<u8>` répétées dans `process_cif_to_frames()`
- **`OnceLock<GfTables>` dans `RsDecoder`** : tables GF(2^8) calculées une seule fois au premier appel et partagées entre toutes les instances — `RsDecoder::new()` est O(1)
- **`SuperframeFilter` : buffer circulaire** : remplace le `copy_within(frame_len.., 0)` (décallage de 4×frame_len octets à chaque frame) par un write_head circulaire — zéro copie sur le chemin d'écriture
- **`pack_bits()` LLVM-vectorisable** : réécriture avec `chunks_exact(8).fold()` — LLVM auto-vectorise en SSSE3/NEON

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
man ./dabctl.1

# Installer system-wide
sudo install -m 644 dabctl.1 /usr/local/share/man/man1/
sudo mandb
man dabctl
```

---

## 🔗 Références

- [eti-stuff](https://github.com/JvanKatwijk/eti-stuff) — Implémentation C++ de référence (IQ → ETI)
- [dablin](https://github.com/Opendigitalradio/dablin) — Décodeur ETI → audio (C++, base du port `eti2pcm`)
- [rtl-sdr](https://github.com/osmocom/rtl-sdr) — Driver RTL-SDR
- [ETSI EN 300 401](https://www.etsi.org/deliver/etsi_en/300400_300499/300401/) — Norme DAB
- [ETSI TS 102 563](https://www.etsi.org/deliver/etsi_ts/102500_102599/102563/) — Norme DAB+
