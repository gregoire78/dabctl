# Guide de compilation pour ARM64

Ce document explique comment compiler `eti-rtlsdr-rust` pour l'architecture ARM64 (aarch64).

## Cross-compilation depuis x86_64

### Installation de rust-tools pour ARM64

```bash
# Installer le toolchain ARM64
rustup target add aarch64-unknown-linux-gnu

# Installer les dépendances GCC cross (optionnel mais recommandé)
sudo apt install -y gcc-aarch64-linux-gnu
```

### Compilation cross-platform

```bash
cd /workspaces/eti-rtl-rust

# Compiler pour ARM64
cargo build --release --target aarch64-unknown-linux-gnu
```

Le binaire sera disponible à:
```
target/aarch64-unknown-linux-gnu/release/eti-rtlsdr-rust
```

## Compilation native sur ARM64

Si vous compilez directement sur une machine ARM64 (Raspberry Pi, etc.):

```bash
# Les étapes sont identiques à x86_64
cargo build --release

# Le binaire sera dans
target/release/eti-rtlsdr-rust
```

## Optimisations pour Raspberry Pi

Si vous compilez pour un Raspberry Pi avec ARMv7 ou ARMv8:

### Pour ARMv8 (Raspberry Pi 3/4 64-bit)

```bash
cargo build --release --target aarch64-unknown-linux-gnu
```

### Pour ARMv7 (Raspberry Pi 2/3 32-bit)

```bash
rustup target add armv7-unknown-linux-gnueabihf
sudo apt install -y gcc-arm-linux-gnueabihf
cargo build --release --target armv7-unknown-linux-gnueabihf
```

## Configuration de Cargo pour optimisation ARM

Vous pouvez créer un fichier `.cargo/config.toml` pour optimiser les compilations ARM:

```toml
[build]
target-dir = "target"

[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

## Dépendances système pour ARM64

Sur un Raspberry Pi ou autre machine ARM64 Linux:

```bash
sudo apt install -y cmake libusb-1.0-0-dev pkg-config build-essential libclang-dev cargo rustc
```

## Distribution du binaire

Une fois compilé, vous pouvez copier le binaire ARM64 sur votre appareil cible:

```bash
# Depuis votre machine de compilation
scp target/aarch64-unknown-linux-gnu/release/eti-rtlsdr-rust \
    user@raspberry-pi:/usr/local/bin/eti-rtlsdr-rust

# Sur le Raspberry Pi
ssh user@raspberry-pi
sudo chmod +x /usr/local/bin/eti-rtlsdr-rust
```

## Troubleshooting

### Erreur de compilation croisée

Si vous rencontrez des erreurs lors de la cross-compilation, assurez-vous que:

1. Le toolchain ARM64 est installé: `rustup target list | grep aarch64`
2. Les dépendances de développement ARM64 sont installées
3. `pkg-config` peut trouver libusb-1.0:

```bash
pkg-config --cflags --libs libusb-1.0
```

### Liens dynamiques manquants

Lors de l'exécution sur ARM64, si vous rencontrez des erreurs de bibliothèques manquantes:

```bash
# Vérifier les dépendances
ldd ./target/aarch64-unknown-linux-gnu/release/eti-rtlsdr-rust

# Installer les dépendances manquantes
sudo apt install -y libusb-1.0-0
```

## Performance

- ARM64 sur Raspberry Pi 4: ~10-20 MHz de traitement en temps réel (suffisant pour DAB)
- x86_64: Bien meilleur, mais overkill pour la plupart des applications DAB

## Ressources

- [Rust Platform Support](https://forge.rust-lang.org/release/platform-support.html)
- [Cross-compilation avec Rust](https://rust-lang.github.io/rustup/cross-compilation.html)
