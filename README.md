# eti-rtlsdr-rust

Un programme CLI en Rust pour extraire les trames ETI (Ensemble Transport Interface) depuis des appareils RTL-SDR. Cet outil est compatible avec `dablin` et peut être utilisé en pipeline pour la réception DAB.

## Caractéristiques

- Supports pour les appareils RTL-SDR (RTL2832U)
- Extraction des trames ETI depuis les canaux DAB
- Sortie des données brutes sur stdout compatible avec `dablin`
- Mode silencieux pour utilisation en pipeline
- Bindings Rust générés avec `rust-bindgen`
- Configuration pour x86_64 et ARM64

## Prérequis

### Dépendances système

- libusb-1.0-dev
- cmake
- build-essential
- libclang-dev (ou clang)
- pkg-config

Sur Debian/Ubuntu :
```bash
sudo apt install -y cmake libusb-1.0-0-dev pkg-config build-essential libclang-dev
```

## Compilation

```bash
cd /workspaces/eti-rtl-rust
cargo build --release
```

Le binaire compilé sera disponible à :
- Debug: `target/debug/eti-rtlsdr-rust`
- Release: `target/release/eti-rtlsdr-rust`

## Utilisation

### Syntaxe

```
eti-rtlsdr-rust -S -C <CHANNEL> -G <GAIN> [OPTIONS]
```

### Arguments

- `-S` : Mode silencieux (pas d'output sur stderr)
- `-C <CHANNEL>` : Canal DAB (ex: 11C, 5A, 11B, etc.) - **obligatoire**
- `-G <GAIN>` : Gain en dB (0-100) - **obligatoire**
- `-O <OUTPUT>` : Fichier de sortie (par défaut: stdout)
- `[DEVICE_INDEX]` : Index de l'appareil RTL-SDR (par défaut: 0)

### Canaux supportés (BAND III)

Canaux 5A - 13D (174.928 - 235.776 MHz)

Exemples: 5A, 5B, 5C, 5D, 6A, ..., 11C, ..., 13D

### Exemples d'utilisation

#### 1. Afficher l'aide

```bash
./target/release/eti-rtlsdr-rust --help
```

#### 2. Recevoir le canal 11C avec un gain de 80

```bash
export LD_LIBRARY_PATH=$(find target/debug/build -name "librtlsdr.so.0" 2>/dev/null | head -1 | xargs dirname):$LD_LIBRARY_PATH
./target/release/eti-rtlsdr-rust -S -C 11C -G 80
```

#### 3. Enregistrer les données ETI dans un fichier

```bash
./target/release/eti-rtlsdr-rust -C 11C -G 80 > "11C_$(date +%F_%H%M).eti"
```

#### 4. Piping vers dablin (avec dablin installé)

```bash
./target/release/eti-rtlsdr-rust -S -C 11C -G 80 | dablin_gtk -L
```

### Presets terrain (ETI + diagnostics)

#### 5. Dongle RTL-SDR -> ETI fichier + résumés ETI/sec et PRS/sec

```bash
./target/release/eti-rtlsdr-rust \
	-C 11C -G 80 \
	--eti \
	--eti-second-summary \
	--prs-second-summary \
	> "11C_$(date +%F_%H%M)_live.eti"
```

#### 6. Replay IQ -> ETI fichier (boucle infinie, pratique pour debug)

```bash
./target/release/eti-rtlsdr-rust \
	-C 11C -G 80 \
	--iq-file ./captures/11C.iq \
	--loop-input \
	--eti \
	--eti-second-summary \
	--prs-second-summary \
	> "11C_replay.eti"
```

#### 7. Dongle RTL-SDR -> dablin (validation rapide)

```bash
./target/release/eti-rtlsdr-rust \
	-C 11C -G 80 \
	--eti \
	--eti-second-summary \
	--prs-second-summary \
	| dablin_gtk -L
```

#### 8. Replay IQ -> dablin (test reproductible)

```bash
./target/release/eti-rtlsdr-rust \
	-C 11C -G 80 \
	--iq-file ./captures/11C.iq \
	--loop-input \
	--eti \
	--eti-second-summary \
	--prs-second-summary \
	| dablin_gtk -L
```

#### 9. Dongle RTL-SDR -> dablin CLI avec sélection de service

```bash
./target/release/eti-rtlsdr-rust \
	-C 11C -G 80 \
	--eti \
	--eti-second-summary \
	--prs-second-summary \
	| dablin -F -s <service> -p
```

#### 10. Replay IQ -> dablin CLI avec sélection de service

```bash
./target/release/eti-rtlsdr-rust \
	-C 11C -G 80 \
	--iq-file ./captures/11C.iq \
	--loop-input \
	--eti \
	--eti-second-summary \
	--prs-second-summary \
	| dablin -F -s <service> -p
```

Notes pratiques:
- Utiliser `-S` coupe les logs diagnostics (donc pas de `ETI/sec` ni `PRS/sec`).
- Pour un run terrain, laisser le mode verbose et surveiller `PRS/sec` (phase RMS basse et stable) ainsi que `ETI/sec`.

## Installation des dépendances système

### Sur Debian/Ubuntu

```bash
sudo apt install -y cmake libusb-1.0-0-dev pkg-config build-essential libclang-dev
# Pour Rust (s'il n'est pas installé)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Architecture du projet

- `build.rs` : Configuration CMake pour compiler librtlsdr et génération des bindings avec bindgen
- `src/main.rs` : Programme principal avec CLI et gestion de l'interface RTL-SDR
- `src/rtlsdr_sys.rs` : Wrapper pour les bindings générés
- `Cargo.toml` : Dépendances Rust et configuration du projet

## Fonctionnement

1. **Compilation de librtlsdr** : Au moment du build, CMake compile librtlsdr depuis les sources du répertoire `rtl-sdr/`
2. **Génération des bindings** : `bindgen` génère les bindings Rust pour l'API C de librtlsdr
3. **Lecture des données** : Le programme ouvre l'appareil RTL-SDR, configure la fréquence et le gain, puis lit les données
4. **Sortie des trames** : Les données brutes sont écrites sur stdout en temps réel

## Notes techniques

- Les données sortantes sont les données brutes I/Q du récepteur
- Le format est compatible avec `dablin` et autres outils DAB
- Le gain est converti de dB vers l'échelle interne RTL-SDR (0-49)
- Le taux d'échantillonnage est fixé à 2.048 MHz (standard DAB)

## Compatibilité

- **Architectures** : x86_64, ARM64
- **Systèmes** : Linux (GNU/Linux Debian-based)
- **Appareils supportés** : RTL-SDR (RTL2832U), DVB-T dongles

## Limitations actuelles

- Supporte uniquement le mode synchrone de lecture
- Pas de support pour d'autres bandes DAB que BAND III
- Pas de décodage ETI natif (sortie des données brutes uniquement)

## Références et ressources

- [RTL-SDR GitHub](https://github.com/osmocom/rtl-sdr)
- [eti-stuff](https://github.com/JvanKatwijk/eti-stuff)
- [dablin](https://github.com/Opendigitalradio/dablin)
- [rust-bindgen Documentation](https://rust-lang.github.io/rust-bindgen/)

## Documentation complémentaire

- [ETI.md](ETI.md) : explication rapide de ce qu'est ETI, différence IQ vs ETI, et état actuel de l'implémentation

## Licence

Ce projet utilise `librtlsdr` qui est sous license GPL 2.0. Ce wrapper Rust suit les mêmes conditions.

## Troubleshooting

### Erreur : "librtlsdr.so.0 not found"

Assurez-vous que `LD_LIBRARY_PATH` pointe vers le répertoire de la bibliothèque :

```bash
export LD_LIBRARY_PATH=$(find target -name "librtlsdr.so.0" 2>/dev/null | head -1 | xargs dirname):$LD_LIBRARY_PATH
```

### Erreur : "No RTL-SDR devices found"

- Vérifiez que l'appareil RTL-SDR est connecté en USB
- Utilisez `lsusb` pour vérifier la détection
- Vérifiez les permissions USB avec `lsusb -d 0bda:2832`

### Gain trop bas/élevé

Ajustez le paramètre `-G` entre 0 et 100. Valeurs recommandées : 60-90 dB

## Auteur

Généré avec assistance IA pour démonstration d'un CLI RTL-SDR en Rust compatible DAB/ETI
