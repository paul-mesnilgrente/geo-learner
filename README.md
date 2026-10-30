# GeoLearner

Application de bureau pour apprendre les pays du monde sur une carte, au clavier ou à la voix.
Écrite entièrement en Rust, avec un seul code pour Linux et Windows, et tout se passe hors ligne.

## Modes de jeu

| Mode | Principe |
|---|---|
| **Tous les pays** | Citer tous les pays de la zone, dans n'importe quel ordre. Un nom reconnu est validé dès qu'il est tapé, sans appuyer sur Entrée. |
| **Nommer le pays** | Un pays s'allume sur la carte : il faut donner son nom. |
| **Trouver sur la carte** | Un nom s'affiche : il faut cliquer sur le bon pays. |

En « Nommer » et « Trouver », on a trois essais par pays. Le score vaut 1 point au premier essai,
½ au deuxième et ¼ au troisième.

Zones jouables :
- le monde entier ;
- les six continents ;
- leurs sous-régions (Balkans, Caraïbes, Asie centrale, Polynésie…) ;
- des regroupements libres : Petites Antilles, Grandes Antilles, Union européenne, îles africaines.

La liste compte 197 pays : les 193 membres de l'ONU, plus le Vatican, la Palestine, le Kosovo
et Taïwan. Elle se modifie dans `data/countries.toml`.

## Saisie vocale

Clic sur le micro (ou Ctrl+M) pendant une partie, puis on énumère les pays à voix haute en
marquant une courte pause entre deux noms. Tout est fait localement :

1. **Capture** : le micro est lu avec `cpal`, ramené en mono et ré-échantillonné à 16 kHz.
2. **Découpage** : la détection de parole repose sur l'énergie du signal, avec un seuil qui
   s'adapte au bruit de fond.
3. **Transcription** de chaque phrase par le moteur choisi.
4. **Recherche des noms de pays**, qui tolère de petites fautes (« État-Unis » → États-Unis).

| Moteur | Modèle | Taille | Principe |
|---|---|---|---|
| Whisper small | `ggml-small-q5_1` (whisper.cpp) | 190 Mo | Transcription libre, le plus robuste en général |
| Whisper base | `ggml-base-q5_1` (whisper.cpp) | 60 Mo | Idem, plus rapide et moins précis |
| Vosk | `vosk-model-small-fr-0.22` (via `utter`) | 42 Mo | Grammaire limitée aux noms de pays, instantané |

Les modèles se téléchargent depuis l'écran d'accueil, avec le `curl` du système.

Pour le diagnostic :
- `GEO_VOICE_DUMP=<dossier>` enregistre chaque phrase captée (fichier WAV et sa transcription) ;
- `GEO_VOICE_SAMPLES=<dossier> cargo test -p geo-learner --release -- --ignored samples --nocapture`
  repasse ces fichiers dans chaque moteur installé ;
- `GEO_WHISPER_PROMPT` remplace l'amorce donnée à Whisper.

## Données locales

| Système | Dossier |
|---|---|
| Linux | `~/.local/share/geo-learner/` |
| Windows | `%APPDATA%\geo-learner\` |

Contenu :
- `progress.json` : historique des parties ;
- `models/` : modèles vocaux ;
- `settings.json` : moteur vocal choisi.

## Compiler et lancer

Prérequis :

- [Rust](https://rustup.rs) (édition 2024, Rust ≥ 1.93) ;
- CMake et un compilateur C++, pour whisper.cpp. Sous Windows, les « Build Tools » de
  Visual Studio fournissent les deux.
- sous Linux, les en-têtes ALSA pour le micro : `sudo apt install libasound2-dev pkg-config cmake`.

`.cargo/config.toml` force une compilation portable de whisper.cpp (`GGML_NATIVE=OFF`, AVX2) :
l'exécutable fonctionne ainsi sur d'autres processeurs que celui de la machine de build.

```sh
cargo run --release -p geo-learner     # lancer l'application
cargo test --workspace                 # tests
```

Le binaire release fait environ 12 Mo. La carte et les polices sont intégrées, pas les modèles vocaux.

## Organisation

```
crates/geo-core   données, normalisation des réponses, logique de quiz, projection (sans interface)
crates/app        application Iced : carte (canvas), écrans, saisie vocale, sauvegarde
crates/mapgen     outil de génération de assets/world.bin depuis Natural Earth
data/             liste des pays (noms, alias, formes orales) et définition des zones
assets/           carte générée et police Inter (sous-ensemble latin)
```

Pour régénérer la carte après une modification de `data/` : `scripts/build-map.sh`.

### Captures en développement

Les builds debug acceptent une partie de démonstration et une capture automatique, ce qui est
utile pour vérifier un rendu sans cliquer :

```sh
GEO_DEMO="mode=free;region=europe;answers=FRA,DEU" GEO_SHOT=/tmp/shot.png cargo run -p geo-learner
```

## Choix techniques

- **Interface** : [Iced](https://iced.rs) 0.14, rendu GPU (wgpu), avec repli sur le rendu
  logiciel `tiny-skia`. Le rendu logiciel seul était plus léger d'environ 3 Mo, mais il laissait
  des zones mal redessinées au survol des boutons.
- **Carte** : Natural Earth 1:50m (domaine public), en projection Natural Earth. Elle existe en
  deux niveaux de détail selon le zoom. Les micro-États sont signalés par des pastilles cliquables.
- **Audio** : `cpal` (ALSA ou PipeWire sous Linux, WASAPI sous Windows).
- **Polices** : Inter (licence OFL, voir `assets/fonts/LICENSE-Inter.txt`).
