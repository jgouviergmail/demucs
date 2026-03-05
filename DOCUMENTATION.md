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

### CLI

#### Backend Vulkan (défaut sur Windows/Linux)

```bash
cargo build -p demucs-cli --release
```

#### Backend CUDA (recommandé pour les GPU NVIDIA)

```bash
cargo build -p demucs-cli --release --features cuda
```

Exploite directement CUDA via le toolkit NVIDIA. Performances optimales sur les cartes GeForce/RTX/Quadro.

#### Backend CPU (sans GPU)

```bash
cargo build -p demucs-cli --release --features cpu
```

Aucune accélération GPU. Beaucoup plus lent, mais fonctionne sur n'importe quelle machine.

Le binaire compilé se trouve dans : `target/release/demucs.exe` (Windows) ou `target/release/demucs` (Linux/macOS).

### Interface graphique

```bash
# Avec accélération CUDA (NVIDIA RTX)
cargo build --release -p demucs-gui --features cuda

# Avec Vulkan (par défaut)
cargo build --release -p demucs-gui

# CPU uniquement
cargo build --release -p demucs-gui --features cpu
```

L'exécutable est dans `target/release/demucs-gui.exe`.

---

## Utilisation CLI

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

## Exemples d'utilisation CLI

### Séparation complète (4 stems par défaut)

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
- Avec le trim des silences activé (GUI) : un fichier `_trimmed.wav` supplémentaire est généré.

---

## Interface graphique (demucs-gui)

Application Windows autonome avec interface egui. Aucune dépendance externe requise — un seul `.exe`.

### Fonctionnalités

- **Drag-and-drop** ou sélection de fichier via dialogue natif
- **Sélection du modèle** avec descriptions (htdemucs, htdemucs_6s, htdemucs_ft)
- **Choix des stems** à extraire (checkboxes dynamiques selon le modèle)
- **Dossier de sortie** configurable (éditable + dialogue dossier)
- **Trim des silences** — paramétrable (durée minimale, remplacement, seuil)
- **Progression détaillée** — chunk, étape, pourcentage, temps écoulé
- **Annulation** à tout moment
- **Cache modèle** — pas de rechargement si même modèle
- **Vue résultats** avec spectrogrammes, lecture multi-pistes et effets audio

### Vue résultats

Après la séparation, l'interface affiche :

- Le **spectrogramme** du fichier original et de chaque stem
- Un **lecteur audio multi-pistes** avec contrôles de transport (lecture, pause, seek, raccourcis clavier)
- Des **contrôles par stem** : solo, mute, gain
- Un **panneau d'effets audio** collapsible par stem (voir section suivante)
- Un bouton **Exporter le mix** pour générer un fichier WAV avec tous les réglages appliqués

### Raccourcis clavier

| Touche | Action |
|--------|--------|
| Espace | Lecture / pause |
| Flèche gauche | Reculer de 5 secondes |
| Flèche droite | Avancer de 5 secondes |
| Clic sur spectrogramme | Seek à la position cliquée |

---

## Effets audio (GUI)

L'interface graphique propose un ensemble d'effets audio par stem, accessibles via un panneau **Effets** collapsible sous chaque piste. Ces effets s'appliquent en temps réel pendant la lecture et sont également pris en compte lors de l'export du mix.

### Vue d'ensemble des effets

| Effet | Type | Description rapide |
|-------|------|-------------------|
| Noise Gate | Nettoyage | Atténue les artefacts de séparation sous un seuil |
| Pan L/R | Spatialisation | Positionne la piste dans le champ stéréo |
| Phase inversée | Utilitaire | Inverse la polarité du signal |
| EQ 3 bandes | Tonalité | Ajuste les graves, médiums et aigus |
| Reverb | Effet spatial | Ajoute de la réverbération via un bus partagé |
| Delay | Effet temporel | Ajoute un écho avec contrôle de feedback |

### Chaîne de traitement

Les effets sont appliqués dans un ordre précis pour chaque stem, suivant les conventions d'une console de mixage :

```
Signal brut
  → Noise Gate (nettoyage)
  → Phase inversée
  → EQ 3 bandes
  → Gain (fader)
  → Pan L/R
  → Delay
  → Accumulation dans le mix + envoi vers le bus reverb
  → [Après tous les stems] Traitement reverb → ajout au mix
  → Gain master → limitation [-1, +1]
```

### Noise Gate

**Ce qu'il fait :** Atténue automatiquement le signal lorsqu'il passe en dessous d'un certain seuil. Utile pour nettoyer les petits artefacts ou le "bleed" (fuite d'instruments voisins) que la séparation Demucs peut laisser sur certaines pistes.

**Paramètres :**

| Paramètre | Plage | Défaut | Description |
|-----------|-------|--------|-------------|
| Activé | on/off | off | Active ou désactive le gate |
| Seuil | -60 à -20 dB | -40 dB | Niveau en dessous duquel le signal est atténué |

**Comment le régler :**

1. Activer le gate sur la piste qui contient des artefacts (souvent les voix ou le piano)
2. Mettre la piste en **solo** pour écouter isolément
3. Ajuster le seuil progressivement :
   - **Trop bas** (vers -60 dB) : le gate ne s'active quasiment pas, les artefacts restent
   - **Trop haut** (vers -20 dB) : le gate coupe des parties du signal utile, le son devient haché
   - **Optimal** : les artefacts dans les silences disparaissent, les passages joués restent intacts
4. Le gate utilise un **soft knee** (transition douce) et un lissage du gain pour éviter les clics

### Pan L/R

**Ce qu'il fait :** Positionne la piste dans le champ stéréo, de l'extrême gauche à l'extrême droite. Utilise une loi de panoramique **equal-power** (cosinus) qui maintient un volume perçu constant quelle que soit la position.

**Paramètres :**

| Paramètre | Plage | Défaut | Description |
|-----------|-------|--------|-------------|
| Pan | -1.0 (L) à +1.0 (R) | 0.0 (centre) | Position dans le champ stéréo |

**Comment le régler :**

- **0.0** : la piste reste au centre (comportement par défaut, identique au signal original)
- **Valeurs négatives** : le son se déplace vers la gauche
- **Valeurs positives** : le son se déplace vers la droite
- Cas d'usage typique : séparer spatialement deux instruments qui occupent la même zone fréquentielle (ex : guitare légèrement à gauche, piano légèrement à droite)

### Phase inversée

**Ce qu'il fait :** Inverse la polarité du signal (multiplie par -1). L'effet n'est pas audible seul, mais devient significatif en combinaison avec d'autres pistes.

**Paramètres :**

| Paramètre | Plage | Défaut | Description |
|-----------|-------|--------|-------------|
| Phase inversée | on/off | off | Inverse la polarité |

**Comment l'utiliser :**

- **Vérification de la séparation** : inverser la phase d'un stem et le mixer avec l'original. Si la séparation est parfaite, le stem s'annule complètement. Le résidu audible correspond à ce que Demucs n'a pas réussi à séparer.
- **Correction de phase** : si deux pistes semblent "creuses" ou "fines" quand elles sont mixées ensemble, inverser la phase de l'une peut améliorer le son (indique un problème de phase dans l'enregistrement original).

### EQ 3 bandes

**Ce qu'il fait :** Égaliseur trois bandes qui permet d'ajuster la tonalité de chaque piste en boostant ou coupant les graves, les médiums et les aigus. Basé sur des filtres biquad (Audio EQ Cookbook) de qualité studio.

**Paramètres :**

| Paramètre | Type de filtre | Fréquence | Plage | Défaut |
|-----------|---------------|-----------|-------|--------|
| EQ activé | — | — | on/off | off |
| Grave | Low Shelf | 200 Hz | -12 à +12 dB | 0 dB |
| Médium | Peaking | 1 kHz | -12 à +12 dB | 0 dB |
| Aigu | High Shelf | 5 kHz | -12 à +12 dB | 0 dB |

**Comment le régler :**

- **Grave (200 Hz)** — Low Shelf : affecte tout ce qui est en dessous de 200 Hz
  - Booster : ajoute du corps, de la chaleur (utile pour une basse un peu fine)
  - Couper : réduit le grondement, clarifie le mix (utile si la batterie ou le "other" a trop de basses)

- **Médium (1 kHz)** — Peaking : affecte une bande centrée sur 1 kHz
  - Booster : fait ressortir la présence, la clarté des voix ou guitares
  - Couper : réduit le côté "nasillard" ou "boxy" d'un instrument

- **Aigu (5 kHz)** — High Shelf : affecte tout ce qui est au-dessus de 5 kHz
  - Booster : ajoute de la brillance, de l'air (cymbales, sibilances vocales)
  - Couper : adoucit le son, réduit la dureté ou les artefacts haute fréquence de la séparation

**Astuce :** Un boost/cut de 3-6 dB est généralement suffisant pour un ajustement subtil. Au-delà de 9 dB, l'effet devient très prononcé.

### Reverb

**Ce qu'il fait :** Ajoute une réverbération artificielle qui simule l'acoustique d'un espace. La reverb fonctionne en **bus partagé** : tous les stems envoient vers une seule instance de reverb (algorithme Freeverb), et le résultat est ajouté au mix global. C'est le même principe qu'un envoi auxiliaire sur une console de mixage.

**Paramètres par stem :**

| Paramètre | Plage | Défaut | Description |
|-----------|-------|--------|-------------|
| Reverb (send) | 0 à 100% | 0% | Quantité de signal envoyée vers le bus reverb |

**Paramètres globaux (section "Reverb globale" en bas de l'interface) :**

| Paramètre | Plage | Défaut | Description |
|-----------|-------|--------|-------------|
| Decay | 0.5 à 5.0 s | 1.5 s | Durée de la queue de réverbération |
| Damping | 0 à 100% | 50% | Absorption des hautes fréquences dans la reverb |

**Comment le régler :**

1. **Reverb send par stem** : commencer à 15-30% sur les pistes souhaitées (voix, guitare). La batterie nécessite généralement moins de reverb (5-15%) pour rester percussive.

2. **Decay** :
   - **0.5-1.0 s** : petite pièce, son intime et serré
   - **1.5-2.5 s** : salle de concert, son naturel et spacieux
   - **3.0-5.0 s** : cathédrale, grand hall, effet très prononcé

3. **Damping** :
   - **0-20%** : reverb brillante, les aigus persistent longtemps (son métallique si excessif)
   - **40-60%** : son naturel et équilibré
   - **80-100%** : reverb sombre et feutrée, les aigus s'éteignent rapidement

**Note :** Le send reverb est **post-fader** — si vous baissez le gain d'un stem, la quantité de reverb envoyée diminue proportionnellement. C'est le comportement standard en mixage.

### Delay

**Ce qu'il fait :** Ajoute un écho (répétition retardée du signal). Chaque stem dispose de son propre delay indépendant. Le delay est un **insert inline post-fader** : le signal retardé est ajouté directement à la piste, et le résultat peut également alimenter le bus reverb.

**Paramètres :**

| Paramètre | Plage | Défaut | Description |
|-----------|-------|--------|-------------|
| Delay activé | on/off | off | Active ou désactive le delay |
| Send | 0 à 100% | 0% | Volume de l'écho par rapport au signal original |
| Temps | 10 à 1000 ms | 250 ms | Intervalle entre les répétitions |
| Feedback | 0 à 95% | 30% | Proportion du signal retardé qui est ré-injectée |

**Comment le régler :**

1. **Temps** : choisir en fonction du tempo du morceau
   - Pour un delay rythmique : `60000 / BPM` donne la durée d'une noire en ms. Diviser par 2 pour une croche, par 4 pour une double croche.
   - Ex : 120 BPM → noire = 500 ms, croche = 250 ms
   - Valeurs courtes (10-50 ms) : effet de doublage/épaississement ("slapback")
   - Valeurs longues (300-1000 ms) : écho distinct

2. **Send** : 20-40% pour un écho subtil, 60-100% pour un effet très prononcé

3. **Feedback** :
   - **0-20%** : un seul écho, puis silence
   - **30-50%** : quelques répétitions qui s'estompent naturellement
   - **70-95%** : beaucoup de répétitions, effet "mur de son" (attention au feedback excessif qui peut saturer)

**Astuce :** Un delay léger (slapback 80-120 ms, send 20%, feedback 10%) sur les voix donne de la profondeur sans effet d'écho perceptible.

---

## Gain et volume master

### Gain par stem

| Paramètre | Plage | Défaut | Description |
|-----------|-------|--------|-------------|
| Gain | 0 à 200% | 100% | Volume individuel de la piste |

Le gain agit comme un fader de console : il contrôle le niveau du signal après l'EQ et avant le pan. À 100%, le signal est inchangé. À 0%, la piste est silencieuse. Au-delà de 100%, le signal est amplifié (attention à la saturation).

### Volume master

| Paramètre | Plage | Défaut | Description |
|-----------|-------|--------|-------------|
| Master | 0 à 150% | 100% | Volume global du mix |

Le master s'applique après le mixage de tous les stems et la reverb. Le signal final est limité à [-1, +1] pour éviter le clipping numérique.

### Solo et Mute

- **Solo (S)** : n'écouter que cette piste (et les autres pistes en solo). Plusieurs stems peuvent être en solo simultanément.
- **Mute (M)** : couper cette piste du mix. Le stem muté n'envoie pas non plus vers la reverb ou le delay.

---

## Export du mix

Le bouton **Exporter le mix** génère un fichier `mix.wav` dans le dossier de sortie. Ce fichier contient le résultat du mix avec **tous les réglages actuels** appliqués :

- Solo / mute de chaque stem
- Gain, pan, phase de chaque stem
- Gate, EQ, reverb send, delay de chaque stem
- Paramètres de reverb globale (decay, damping)
- Volume master

L'export utilise exactement le même algorithme de traitement que la lecture en temps réel, garantissant que le fichier WAV produit correspond fidèlement à ce que vous entendez.

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

### Pas de son à la lecture (GUI)

L'application utilise la sortie audio par défaut du système via WASAPI (Windows). Vérifiez que :
- Un périphérique de sortie audio est configuré dans les paramètres Windows
- Le volume système n'est pas à zéro
- Aucune autre application ne bloque le périphérique audio en mode exclusif
