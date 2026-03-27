# Format de sortie et spécifications

## Format de sortie

`eti-rtlsdr-rust` sort les données brutes I/Q du récepteur RTL-SDR sur stdout. Le format est:

- **Type de données** : Unsigned 8-bit (u8)
- **Format** : Interleaved I/Q samples
- **Taux d'échantillonnage** : 2.048 MHz (standard DAB)
- **Taille du buffer de lecture** : 16 × 16384 bytes = 262144 bytes par lecture

### Spécifications I/Q

- **I (In-phase)** : Premier byte
- **Q (Quadrature)** : Deuxième byte
- **Ordre** : I0, Q0, I1, Q1, I2, Q2, ...

### Exemple de structure de données

```
Byte 0:   I0 (In-phase sample 0)
Byte 1:   Q0 (Quadrature sample 0)
Byte 2:   I1 (In-phase sample 1)
Byte 3:   Q1 (Quadrature sample 1)
...
Byte 2N:   IN
Byte 2N+1: QN
```

## Compatibilité avec d'autres outils

### DABlin

Le format de sortie est directement compatible avec `dablin`:

```bash
./eti-rtlsdr-rust.sh -S -C 11C -G 80 | dablin_gtk -L
```

### Enregistrement en fichier WAV

Vous pouvez convertir le flux en fichier WAV (16-bit) :

```bash
./eti-rtlsdr-rust.sh -S -C 11C -G 80 | sox -r 2048k -b 8 -e unsigned-integer -c 1 -t raw - output.wav
```

### Traitement avec Python

```python
import sys
import numpy as np

# Lire depuis stdin
data = np.fromfile(sys.stdin.buffer, dtype=np.uint8)

# Convertir en I/Q complexe
# Centrer les valeurs autour de 0 (-128 à +127)
iq_data = data.astype(np.float32) - 128

# Créer les paires I/Q
complex_samples = iq_data[0::2] + 1j * iq_data[1::2]

print(f"Total samples: {len(complex_samples)}")
print(f"Duration at 2.048 MHz: {len(complex_samples) / 2.048e6} seconds")
```

## Performance et bande passante

- **Débit brut** : 2 bytes/sample × 2.048 MHz = ~4.096 MB/s
- **Débit effectif** : ~4.1 MB/s en continu
- **Une heure d'enregistrement** : ~14.8 GB

## Contrôle du volume

Ajustez le gain pour améliorer la réception :

- **Gain faible (0-30)** : Bruit bas, signal faible, utiliser pour les signaux forts
- **Gain moyen (40-70)** : Meilleur compromis
- **Gain élevé (80-100)** : Plus de sensibilité, mais plus de bruit

## Paramètres de fréquence DAB

### Bande III officielle (L-band DAB)

Les fréquences sont en Hz :

| Canal | Fréquence |
|-------|-----------|
| 5A | 174,928 MHz |
| 5B | 176,640 MHz |
| 5C | 178,352 MHz |
| 5D | 180,064 MHz |
| 6A | 181,936 MHz |
| 11A | 216,928 MHz |
| 11B | 218,640 MHz |
| 11C | 220,352 MHz |
| 11D | 222,064 MHz |
| 12A | 223,936 MHz |
| 13A | 230,784 MHz |
| 13B | 232,496 MHz |
| 13C | 234,208 MHz |
| 13D | 235,776 MHz |

## Débogage

### Vérifier la sortie en brut

```bash
# Sortir 1000 bytes et les afficher en hexadécimal
./eti-rtlsdr-rust.sh -S -C 11C -G 80 2>/dev/null | head -c 1000 | xxd | head
```

### Vérifier le débit

```bash
# Mesurer le débit en bytes par seconde
time (./eti-rtlsdr-rust.sh -S -C 11C -G 80 2>/dev/null | head -c 10000000 > /dev/null)
# Devrait être environ 2.5 secondes pour 10 MB = 4 MB/s
```

### Enregistrer et analyser

```bash
# Enregistrer 1 seconde de données
./eti-rtlsdr-rust.sh -S -C 11C -G 80 2>/dev/null | head -c 4000000 > sample.raw

# Vérifier la taille
ls -lh sample.raw
```

## Signaux d'interruption

Le programme réagit aux signaux :

- **Ctrl+C (SIGINT)** : Arrêt gracieux
- **SIGTERM** : Arrêt gracieux

Les données en cours de transmission sont vidées avant l'arrêt.

## Notes techniques

- La conversion ADC du RTL-SDR est 8-bit
- Les valeurs I/Q brutes sont centrées autour de 128 (plage 0-255)
- Pour un traitement numérique, soustrayez 128 pour obtenir une plage signée (-128 à +127)
- La fréquence d'échantillonnage est fixe à 2.048 MHz

## Limitations

- Pas de support d'autres formats d'échantillonnage
- Pas de compression
- Les données perdues ne sont pas détectées
