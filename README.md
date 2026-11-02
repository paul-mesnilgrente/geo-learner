# GeoLearner

Application de bureau pour apprendre les pays du monde sur une carte, au clavier ou à la voix.
Écrite entièrement en Rust, avec un seul code pour Linux et Windows, et tout se passe hors ligne.

## Quiz

| Catégorie | Quiz | Lieux | Zones |
|---|---|---|---|
| Monde | Pays du monde | 197 pays | continents, sous-régions, regroupements |
| France | Départements français | 101 départements | métropole, outre-mer, régions |
| France | Régions françaises | 18 régions | métropole, outre-mer |
| États-Unis | États des États-Unis | 50 États | régions et divisions du recensement |

L'écran d'accueil présente le catalogue dans une colonne à gauche, groupé par catégorie : la
liste peut s'allonger sans changer l'interface. Les textes s'accordent au quiz (« Nommer la
région », « 13 régions trouvées », « Quel est cet État ? »).

Les départements et régions d'outre-mer, l'Alaska et Hawaï sont déplacés en **encarts** près de
la carte principale, dans un cadre. Les numéros de département sont acceptés au clavier
(« 75 », « 2A », « 971 »), et les noms anglais des États (« California »).

### Ajouter un quiz

Un quiz de subdivisions (provinces, Länder…) se décrit dans un fichier `data/atlases/<id>.toml`
(voir ceux existants) :
- `[atlas]` : titre, catégorie, noms au singulier et au pluriel, genre, pays Natural Earth,
  cadre des terres voisines ;
- `[[inset]]` : encarts ;
- `[world]`, `[[continent]]`, `[[sub]]` : zones ;
- `items` : les lieux, avec leurs codes ISO 3166-2. `parts` réunit plusieurs entités en une
  seule, comme une région faite de ses départements.

Ensuite :
1. lancer `scripts/build-map.sh` ;
2. ajouter le fichier `.bin` produit à `ATLASES`, dans `crates/app/src/main.rs`.

## Modes de jeu

| Mode | Principe |
|---|---|
| **Tous les pays** | Citer tous les pays de la zone, dans n'importe quel ordre. Un nom reconnu est validé dès qu'il est tapé, sans appuyer sur Entrée. |
| **Nommer le pays** | Un pays s'allume sur la carte : il faut donner son nom. |
| **Trouver sur la carte** | Un nom s'affiche : il faut cliquer sur le bon pays. |

Un **minuteur** facultatif s'active sur l'écran d'accueil. Par défaut, il dure 15 minutes pour
citer les 197 pays du monde, au prorata du nombre de pays de la zone, et un peu plus par pays en
« Nommer » (7 s) et « Trouver » (9 s). La durée se règle pour chaque mode et chaque zone. À zéro,
la partie est validée : les pays restants comptent comme ratés.

Sur la carte, les archipels (Maldives, Kiribati, Fidji, Cap-Vert…) sont entourés d'une **bulle**
cliquable, comme sur JetPunk, pour être repérables malgré leur taille. Il y a une bulle par
groupe d'îles proches : leur enveloppe, lissée, avec une marge de quelques pixels qui se réduit
près d'une autre terre (les Bahamas ne touchent pas la Floride). `one_bubble` force une seule
bulle (Micronésie). Les îles qui dépassent un bord de la carte (îles de la Ligne de Kiribati,
Aléoutiennes) réapparaissent de l'autre côté.

En « Nommer » et « Trouver », on a trois essais par pays. Le score vaut 1 point au premier essai,
½ au deuxième et ¼ au troisième.

Zones jouables :
- le monde entier ;
- les six continents ;
- leurs sous-régions (Balkans, Caraïbes, Asie centrale, Polynésie…) ;
- des regroupements libres : Petites Antilles, Grandes Antilles, Union européenne, îles africaines.

Sur l'écran d'accueil, un **aperçu** montre la zone choisie, cadrée comme en partie, avec ses pays
en bleu.

La liste compte 197 pays : les 193 membres de l'ONU, plus le Vatican, la Palestine, le Kosovo
et Taïwan. Elle se modifie dans `data/countries.toml`.

## Saisie vocale

Clic sur le micro (ou Ctrl+M) pendant une partie, puis on énumère les pays à voix haute, à la
suite. Chaque pays est validé dès que deux transcriptions successives de la phrase en cours
s'accordent sur lui. Un nom qui peut encore s'allonger (« Niger » → « Nigeria ») attend le mot
suivant ou la fin de la phrase. Tout est fait localement :

1. **Capture** : le micro est lu avec `cpal`, ramené en mono et ré-échantillonné à 16 kHz.
2. **Découpage** : la détection de parole repose sur l'énergie du signal, avec un seuil qui
   s'adapte au bruit de fond.
3. **Transcription** de la phrase en cours, toutes les 0,3 s environ.
4. **Recherche des noms de pays**, qui tolère de petites fautes (« État-Unis » → États-Unis).

La reconnaissance utilise [Vosk](https://alphacephei.com/vosk/) via `utter`, un décodeur en pur
Rust, avec le modèle `vosk-model-small-fr-0.22` (42 Mo) et une grammaire limitée aux noms de pays.
Whisper a été essayé puis retiré : en transcription libre, il comprend mal les noms isolés
(« Uruguay » devient « Le rodoin »), et il alourdissait l'application.

Le modèle se télécharge depuis l'écran d'accueil, avec le `curl` du système. Son empreinte
SHA-256 est vérifiée avant installation : un fichier corrompu ou modifié est rejeté.

Pour le diagnostic :
- `GEO_VOICE_DUMP=<dossier>` enregistre chaque phrase captée (fichier WAV et sa transcription) ;
- `GEO_VOICE_SAMPLES=<dossier> cargo test -p geo-learner --release -- --ignored samples --nocapture`
  repasse ces fichiers dans le moteur ;
- `GEO_VOICE_WAV=<fichier> cargo test -p geo-learner --release -- --ignored streams --nocapture`
  rejoue un enregistrement en temps réel et montre quand chaque pays est validé.

## Données locales

| Système | Dossier |
|---|---|
| Linux | `~/.local/share/geo-learner/` |
| Windows | `%APPDATA%\geo-learner\` |

Contenu :
- `progress.json` : historique des parties ;
- `settings.json` : minuteur (activé ou non, durées personnalisées) ;
- `models/` : modèle vocal.

## Compiler et lancer

Prérequis :

- [Rust](https://rustup.rs) (édition 2024, Rust ≥ 1.93) ;
- sous Linux, les en-têtes ALSA pour le micro : `sudo apt install libasound2-dev pkg-config`.
  Sous Windows, il faut les « Build Tools » de Visual Studio (compilateur MSVC).

```sh
cargo run --release -p geo-learner     # lancer l'application
cargo test --workspace                 # tests
```

Le binaire release fait environ 11 Mo. La carte et les polices sont intégrées, pas les modèles vocaux.

## Microsoft Store

Le paquet MSIX, les logos, la politique de confidentialité et la fiche du Store sont prêts.
La marche à suivre (compte Partner Center, identité du paquet, soumission) est décrite dans
[`docs/microsoft-store.md`](docs/microsoft-store.md). L'icône est générée à partir des contours
Natural Earth par `scripts/make-icon.py` (Inkscape et ImageMagick).

Sous Windows, l'exécutable intègre le runtime Visual C++ (`crt-static`, voir `.cargo/config.toml`) :
il ne dépend pas de `VCRUNTIME140.dll`, ce que la CI vérifie.

## Intégration continue

`.github/workflows/ci.yml` compile et teste l'application sous **Linux et Windows** à chaque push :
formatage, clippy (sans avertissement), tests, puis compilation release. Les exécutables produits
sont téléchargeables dans l'onglet *Actions* de GitHub (artefacts `geo-learner-Windows` et
`geo-learner-Linux`).

Pour vérifier localement que le code compile pour Windows :

```sh
rustup target add x86_64-pc-windows-msvc
cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc
```

## Organisation

```
crates/geo-core   données, normalisation des réponses, logique de quiz, projection (sans interface)
crates/app        application Iced : carte (canvas), écrans, saisie vocale, sauvegarde
crates/mapgen     génération des atlas (assets/*.bin) depuis Natural Earth (admin-0 et admin-1)
data/             pays du monde et zones ; data/atlases/ : départements, régions, États
assets/           carte générée et police Inter (sous-ensemble latin)
```

Pour régénérer les cartes après une modification de `data/` : `scripts/build-map.sh`.

### Captures en développement

Les builds debug acceptent une partie de démonstration et une capture automatique, ce qui est
utile pour vérifier un rendu sans cliquer :

```sh
GEO_DEMO="mode=free;region=europe;answers=FRA,DEU" GEO_SHOT=/tmp/shot.png cargo run -p geo-learner
```

Paramètres de `GEO_DEMO`, séparés par `;` :
- `mode` et `region` : la partie à lancer ;
- `answers` : codes des pays déjà joués ;
- `target` : premier pays à deviner ;
- `atlas` : quiz à ouvrir (`fr-departements`, `fr-regions`, `us-etats`) ;
- `type=a|b` : saisie simulée au clavier ;
- `zoom=lon,lat,z` : position de la caméra ;
- `elapsed` : temps déjà écoulé, en secondes ;
- `timer` : minuteur imposé, en secondes ;
- `giveup`, `summary` : partie abandonnée, bilan affiché.

`GEO_WINDOW=1600x900` impose la taille de la fenêtre.

## Choix techniques

- **Interface** : [Iced](https://iced.rs) 0.14, rendu GPU (wgpu), avec repli sur le rendu
  logiciel `tiny-skia`. Le rendu logiciel seul était plus léger d'environ 3 Mo, mais il laissait
  des zones mal redessinées au survol des boutons.
- **Carte** : Natural Earth 1:50m (domaine public), en projection Natural Earth. Elle existe en
  deux niveaux de détail selon le zoom. Les micro-États sont signalés par des pastilles cliquables.
- **Audio** : `cpal` (ALSA ou PipeWire sous Linux, WASAPI sous Windows).
- **Polices** : Inter (licence OFL, voir `assets/fonts/LICENSE-Inter.txt`).
