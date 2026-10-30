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

Clic sur le micro (ou Ctrl+M) pendant une partie, puis énumérer les pays à voix haute.
La reconnaissance est faite localement par [`utter`](https://crates.io/crates/utter), un
décodeur en pur Rust pour les modèles Vosk. Il utilise le modèle `vosk-model-small-fr-0.22`
et une grammaire restreinte aux noms de pays, ce qui le rend rapide et précis.

Le modèle (41 Mo) est téléchargé une seule fois, depuis l'application, avec le `curl` du système.
Il est stocké dans le dossier de données de l'application.

## Données locales

| Système | Dossier |
|---|---|
| Linux | `~/.local/share/geo-learner/` |
| Windows | `%APPDATA%\geo-learner\` |

Contenu :
- `progress.json` : historique des parties ;
- `models/` : modèle vocal.

## Compiler et lancer

Prérequis :

- [Rust](https://rustup.rs) (édition 2024, Rust ≥ 1.93) ;
- sous Linux, les en-têtes ALSA pour le micro : `sudo apt install libasound2-dev pkg-config`.
  Windows n'a besoin de rien de plus.

```sh
cargo run --release -p geo-learner     # lancer l'application
cargo test --workspace                 # tests
```

Le binaire release fait environ 8 Mo. La carte et les polices sont intégrées.

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

- **Interface** : [Iced](https://iced.rs) 0.14, avec un rendu logiciel `tiny-skia` (sans GPU),
  ce qui garantit un rendu identique partout et un binaire plus petit.
- **Carte** : Natural Earth 1:50m (domaine public), en projection Natural Earth. Elle existe en
  deux niveaux de détail selon le zoom. Les micro-États sont signalés par des pastilles cliquables.
- **Audio** : `cpal` (ALSA ou PipeWire sous Linux, WASAPI sous Windows).
- **Polices** : Inter (licence OFL, voir `assets/fonts/LICENSE-Inter.txt`).
