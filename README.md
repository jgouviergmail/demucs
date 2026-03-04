# Demucs-RS

Implémentation native Rust de [HTDemucs v4](https://arxiv.org/abs/2211.08553) — séparation de sources audio par intelligence artificielle. Sépare n'importe quel morceau en pistes individuelles (batterie, basse, voix, etc.) avec inférence GPU via [Burn](https://burn.dev).

Basé sur le projet [demucs-rs](https://github.com/nikhilunni/demucs-rs) de Nikhil Unni, avec ajout d'une **interface graphique Windows**, du support **CUDA (RTX)** et du **trim des silences**.

## Fonctionnalités

- **3 modèles** — Standard (4 stems), 6 Stems (+ guitare & piano), Fine-Tuned (meilleure qualité)
- **Accélération GPU** — CUDA (NVIDIA RTX), Vulkan, Metal (macOS), WebGPU
- **Interface graphique** — application Windows autonome (un seul .exe), drag-and-drop, progression en temps réel
- **CLI native** — ligne de commande rapide avec barre de progression
- **Trim des silences** — post-traitement optionnel pour supprimer les longs silences dans les vocals
- **Formats audio** — WAV, MP3, FLAC, OGG, M4A/AAC, AIFF

## Modèles

| Modèle | Stems | Taille | Description |
|--------|-------|--------|-------------|
| `htdemucs` | batterie, basse, autre, voix | 84 Mo | Bon compromis vitesse/qualité |
| `htdemucs_6s` | batterie, basse, autre, voix, guitare, piano | 84 Mo | Séparation étendue |
| `htdemucs_ft` | batterie, basse, autre, voix | 333 Mo | Fine-tuned, meilleure qualité |

Les poids sont téléchargés automatiquement depuis Hugging Face à la première utilisation et mis en cache.

## Interface graphique (demucs-gui)

Application Windows autonome avec interface egui. Aucune dépendance externe requise.

### Fonctionnalités GUI

- **Drag-and-drop** ou sélection de fichier via dialogue natif
- **Sélection du modèle** avec descriptions
- **Choix des stems** à extraire (checkboxes dynamiques selon le modèle)
- **Dossier de sortie** configurable (éditable + dialogue dossier)
- **Trim des silences (vocals)** — paramétrable :
  - Durée minimale de silence pour déclencher le trim (défaut : 2s)
  - Durée du silence de remplacement (défaut : 0.5s)
  - Seuil de détection du silence (défaut : -40 dB)
- **Progression détaillée** — chunk, étape, pourcentage, temps écoulé
- **Annulation** à tout moment
- **Cache modèle** — pas de rechargement si même modèle
- **Ouvrir le dossier** de sortie directement depuis l'interface

### Compilation GUI

```bash
# Avec accélération CUDA (NVIDIA RTX)
cargo build --release -p demucs-gui --features cuda

# Avec Vulkan (par défaut)
cargo build --release -p demucs-gui

# CPU uniquement
cargo build --release -p demucs-gui --features cpu
```

L'exécutable est dans `target/release/demucs-gui.exe`.

### Fichiers de sortie

Les fichiers WAV sont nommés `{source}_{stem}.wav` :
- `chanson.mp3` → `chanson_drums.wav`, `chanson_bass.wav`, `chanson_vocals.wav`, `chanson_other.wav`
- Avec trim activé : `chanson_vocals_trimmed.wav` (en plus de l'original)

## CLI

```
Usage: demucs [OPTIONS] <INPUT>

Arguments:
  <INPUT>  Fichier audio (WAV, AIFF, FLAC, MP3, OGG, M4A/AAC)

Options:
  -m, --model <MODEL>    Modèle [défaut: htdemucs]
                         [valeurs: htdemucs, htdemucs_6s, htdemucs_ft]
  -s, --stems <STEMS>    Stems à extraire, séparés par des virgules
                         Disponibles: drums, bass, other, vocals, guitar, piano
  -o, --output <OUTPUT>  Dossier de sortie [défaut: ./stems/]
      --debug            Statistiques de debug par couche
  -h, --help             Aide
```

### Exemples CLI

```bash
# Séparer toutes les pistes
demucs chanson.mp3

# Extraire uniquement les voix
demucs chanson.mp3 -s vocals

# Modèle 6 stems dans un dossier personnalisé
demucs chanson.flac -m htdemucs_6s -o ./mes_pistes/

# Meilleure qualité avec le modèle fine-tuned
demucs chanson.wav -m htdemucs_ft
```

### Compilation CLI

```bash
# Avec CUDA
cargo build --release -p demucs-cli --features cuda

# Avec Vulkan (par défaut)
cargo build --release -p demucs-cli

# CPU uniquement
cargo build --release -p demucs-cli --features cpu
```

## Prérequis

- **Rust** (toolchain stable)
- **GPU** avec support Vulkan (Windows/Linux) ou Metal (macOS)
- **CUDA Toolkit** (optionnel, pour l'accélération NVIDIA) — variable `CUDA_PATH` requise

### Configuration CUDA (Windows)

```powershell
# Définir CUDA_PATH de manière permanente
[System.Environment]::SetEnvironmentVariable('CUDA_PATH', 'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.x', 'User')
```

Redémarrer le terminal après cette commande.

## Structure du projet

```
demucs-rs/
├── demucs-core/     Bibliothèque ML (modèle, DSP, poids)
├── demucs-cli/      CLI native (clap, symphonia, indicatif)
├── demucs-gui/      Interface graphique Windows (eframe/egui)
├── demucs-plugin/   Plugin DAW — VST3/CLAP (macOS)
├── demucs-wasm/     Adaptateur WebAssembly
├── web/             Frontend React + TypeScript
└── bench/           Benchmarks Python
```

## Crédits

- [demucs-rs](https://github.com/nikhilunni/demucs-rs) par Nikhil Unni — projet original
- [Demucs](https://github.com/facebookresearch/demucs) par Meta Research — implémentation PyTorch originale
- [Burn](https://burn.dev) — framework deep learning Rust

## Licence

[Apache License, Version 2.0](LICENSE)
