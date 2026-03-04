# Demucs-rs — Documentation

Implémentation native en Rust de [HTDemucs v4](https://arxiv.org/abs/2211.08553), un modèle de séparation de sources musicales. Permet d'isoler les pistes individuelles (batterie, basse, voix, etc.) d'un fichier audio.

## Prérequis

### Obligatoire

- **Rust** (toolchain stable) — [https://rustup.rs](https://rustup.rs)

### Selon le backend GPU choisi

| Backend | Système | Prérequis |
|---------|---------|-----------|
| Vulkan (défaut) | Windows / Linux | Pilotes GPU récents (Vulkan 1.2+) |
| CUDA | Windows / Linux | NVIDIA CUDA Toolkit + variable `CUDA_PATH` configurée |
| Metal | macOS | Aucun (inclus dans macOS) |
| CPU | Tous | Aucun (pas d'accélération GPU) |

#### Configuration de CUDA_PATH (backend CUDA uniquement)

Le backend CUDA nécessite que la variable d'environnement `CUDA_PATH` pointe vers le répertoire d'installation du CUDA Toolkit, afin que le compilateur JIT puisse trouver les headers (`cuda_runtime.h`).

**PowerShell (permanent) :**
```powershell
[System.Environment]::SetEnvironmentVariable('CUDA_PATH', 'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.9', 'User')
```

**PowerShell (session courante uniquement) :**
```powershell
$env:CUDA_PATH = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.9"
```

Adaptez le chemin et la version (`v12.9`) à votre installation.

---

## Compilation

### Backend Vulkan (défaut sur Windows/Linux)

```bash
cargo build -p demucs-cli --release
```

### Backend CUDA (recommandé pour les GPU NVIDIA)

```bash
cargo build -p demucs-cli --release --features cuda
```

Exploite directement CUDA via le toolkit NVIDIA. Performances optimales sur les cartes GeForce/RTX/Quadro.

### Backend CPU (sans GPU)

```bash
cargo build -p demucs-cli --release --features cpu
```

Aucune accélération GPU. Beaucoup plus lent, mais fonctionne sur n'importe quelle machine.

Le binaire compilé se trouve dans : `target/release/demucs.exe` (Windows) ou `target/release/demucs` (Linux/macOS).

---

## Utilisation

```
demucs [OPTIONS] <FICHIER_AUDIO>
```

### Argument obligatoire

| Argument | Description |
|----------|-------------|
| `<FICHIER_AUDIO>` | Fichier audio d'entrée. Formats supportés : WAV, AIFF, FLAC, MP3, OGG, M4A/AAC. Accepte le stéréo ou le mono, à n'importe quel taux d'échantillonnage. |

### Options

| Option | Forme courte | Valeur par défaut | Description |
|--------|-------------|-------------------|-------------|
| `--model <MODEL>` | `-m` | `htdemucs` | Modèle à utiliser pour la séparation (voir section Modèles ci-dessous). |
| `--stems <STEMS>` | `-s` | Tous les stems du modèle | Stems à extraire, séparés par des virgules. Ex : `drums,vocals`. |
| `--output <DOSSIER>` | `-o` | `./stems/` | Dossier de sortie pour les fichiers WAV générés. |
| `--debug` | — | désactivé | Affiche les statistiques couche par couche du modèle pendant l'inférence. |
| `--help` | `-h` | — | Affiche l'aide. |

---

## Modèles disponibles

Trois variantes du modèle HTDemucs sont disponibles. Les poids sont téléchargés automatiquement depuis Hugging Face lors de la première utilisation, puis mis en cache localement.

| Modèle | Stems produits | Taille | Description |
|--------|---------------|--------|-------------|
| `htdemucs` | drums, bass, other, vocals | 84 Mo | Modèle standard. Bon compromis vitesse/qualité. |
| `htdemucs_6s` | drums, bass, other, vocals, **guitar**, **piano** | 84 Mo | Ajoute la séparation guitare et piano. |
| `htdemucs_ft` | drums, bass, other, vocals | 333 Mo | Version fine-tunée. Meilleure qualité, mais plus lent (exécute un modèle par stem). |

---

## Stems disponibles

| Stem | Disponible dans |
|------|----------------|
| `drums` | htdemucs, htdemucs_6s, htdemucs_ft |
| `bass` | htdemucs, htdemucs_6s, htdemucs_ft |
| `other` | htdemucs, htdemucs_6s, htdemucs_ft |
| `vocals` | htdemucs, htdemucs_6s, htdemucs_ft |
| `guitar` | htdemucs_6s uniquement |
| `piano` | htdemucs_6s uniquement |

---

## Exemples d'utilisation

### Séparation complète (4 stems par défaut)

D:\Developpement\Demucs\demucs-rs\target\release
.\demucs.exe kaya.mp3


```bash
demucs chanson.mp3
```

Produit 4 fichiers WAV dans `./stems/` : `drums.wav`, `bass.wav`, `other.wav`, `vocals.wav`.

### Extraire uniquement les voix

```bash
demucs chanson.mp3 -s vocals
```

Produit uniquement `./stems/vocals.wav`.

### Extraire batterie et basse

```bash
demucs chanson.mp3 -s drums,bass
```

### Utiliser le modèle 6 stems

```bash
demucs chanson.flac -m htdemucs_6s
```

Produit 6 fichiers : drums, bass, other, vocals, guitar, piano.

### Meilleure qualité avec le modèle fine-tuné

```bash
demucs chanson.wav -m htdemucs_ft
```

Plus lent car il exécute un sous-modèle dédié par stem, mais offre la meilleure qualité de séparation.

### Spécifier un dossier de sortie

```bash
demucs chanson.mp3 -o ./mes_stems/
```

### Extraire uniquement le piano (modèle 6 stems)

```bash
demucs chanson.mp3 -m htdemucs_6s -s piano
```

### Mode debug (statistiques d'inférence)

```bash
demucs chanson.mp3 --debug
```

Affiche les dimensions des tenseurs et les temps d'exécution pour chaque couche du réseau de neurones.

---

## Format de sortie

- Tous les fichiers produits sont au format **WAV** (PCM 16 bits).
- Le taux d'échantillonnage et le nombre de canaux (stéréo) correspondent à ceux du fichier d'entrée.
- Chaque fichier est nommé d'après le stem : `drums.wav`, `bass.wav`, `vocals.wav`, etc.

---

## Dépannage

### Stack overflow au lancement

Le modèle peut nécessiter une pile d'exécution plus grande que celle par défaut. Le binaire actuel intègre un fix qui alloue automatiquement 64 Mo de pile. Si vous compilez sans ce fix, vous pouvez contourner le problème via la variable d'environnement :

```powershell
$env:RUST_MIN_STACK = 16777216   # 16 Mo
.\demucs.exe chanson.mp3
```

### Erreur "cannot open source file cuda_runtime.h" (backend CUDA)

La variable `CUDA_PATH` n'est pas définie ou pointe vers un mauvais répertoire. Voir la section [Configuration de CUDA_PATH](#configuration-de-cuda_path-backend-cuda-uniquement).

### Le modèle se retélécharge à chaque exécution

Les poids sont mis en cache dans le répertoire de données de l'utilisateur (géré par la crate `dirs`). Sous Windows : `C:\Users\<utilisateur>\AppData\Local\`. Vérifiez que vous avez les droits d'écriture dans ce répertoire.
