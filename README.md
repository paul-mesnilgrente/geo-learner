# GeoLearner

Application de bureau pour apprendre les pays du monde sur une carte, au clavier ou à la voix.
Écrite entièrement en Rust, avec un seul code pour Linux et Windows, et tout se passe hors ligne.

## Langues

L'interface, les noms des lieux et la reconnaissance vocale existent en **français** et en
**anglais**. Au premier lancement, GeoLearner prend la langue du système (l'anglais si elle n'est
pas prise en charge). Le choix fait sur l'accueil est enregistré dans `settings.json` et vaut
pour les lancements suivants, même si la langue du système change.

- Les textes sont écrits dans chaque langue là où ils servent, avec la macro `tr!`
  (`crates/app/src/i18n.rs`) ; les accords et articles sont dans `words.rs`.
- Les noms viennent des atlas : `Text` contient une traduction par langue. Les noms anglais
  sont ceux de Natural Earth (`NAME_EN`, `name_en`), sauf `en = "..."` dans les fichiers de
  données ; les zones et les titres des atlas ont leur `en` / `title_en`.
- Toutes les langues sont acceptées au clavier, quelle que soit celle de l'interface.
- La voix utilise le petit modèle Vosk de la langue (`vosk-model-small-en-us-0.15`, 41 Mo, pour
  l'anglais), téléchargé à la demande et vérifié par SHA-256. Le modèle anglais reconnaît tous
  les pays, États, provinces et territoires, mais pas tous les noms français des départements
  (72 sur 101) : ceux-là se tapent.

Ajouter une langue : une variante dans `Lang` (`geo-core`), un argument à chaque `tr!` (le
compilateur signale les oublis), les noms dans les données, et le modèle Vosk dans
`voice/models.rs`.

## Quiz

| Catégorie | Quiz | Lieux | Zones |
|---|---|---|---|
| Monde | Pays du monde | 197 pays | continents, sous-régions, regroupements |
| Monde | Drapeaux du monde | 197 drapeaux | comme les pays du monde |
| Monde | Océans | 5 océans | monde entier |
| Monde | Mers du monde | 52 mers | océans (Atlantique, Pacifique, Indien, Arctique, Austral) et régions |
| Monde | Fleuves du monde | 55 fleuves | continents |
| France | Départements français | 101 départements | métropole, outre-mer, régions |
| France | Numéros des départements (sans carte) | 101 départements | métropole, outre-mer, régions |
| France | Régions françaises | 18 régions | métropole, outre-mer |
| États-Unis | États des États-Unis | 50 États | régions et divisions du recensement |
| Canada | Provinces et territoires du Canada | 13 | Atlantique, Centre, Prairies, Côte Ouest, Nord |
| Australie | États et territoires d'Australie | 8 | États, territoires |

**Numéros des départements** se joue sans carte, dans les deux sens : « Numéro → nom » (le
numéro s'affiche en grand, on tape ou dit le nom) et « Nom → numéro » (on tape le numéro, sans
micro). Les noms ne valident pas « Nom → numéro » et inversement. Ce quiz est dérivé à
l'exécution de l'atlas des départements : il n'ajoute rien à l'exécutable. En fin de partie,
toutes les paires numéro · nom s'affichent, les ratées en rouge.

Pour apprendre les numéros par petits paquets, ce quiz a aussi une zone par dizaine
(« Par numéro » : 01–09, 10–19, …, 90–95, 971–976). Les **Départements français** (avec carte)
les ont aussi, en plus de « Métropole » (et ses 13 régions) et « Outre-mer ». Une zone qui
s'ouvre sur des sous-zones porte un chevron. L'exploration des départements rappelle la
logique des numéros (l'ordre alphabétique des noms d'origine), met en jaune un repère par
dizaine (01 Ain, 10 Aube, 21 Côte-d'Or… 90 Territoire de Belfort) et liste les exceptions.

L'Australie compte six États et deux territoires continentaux (Territoire du Nord, Territoire de
la capitale australienne) ; l'île Lord Howe est rattachée à la Nouvelle-Galles du Sud. Les
contours du Canada sont un peu plus simplifiés que les autres (`tolerance` dans le fichier de
l'atlas) : les îles de l'Arctique doubleraient sinon la taille du fichier.

L'**accueil** présente les quiz en cartes (miniature, catégorie, dernière partie), avec les
dernières parties à droite : un clic sur l'une d'elles rouvre son quiz, réglé sur le même mode et
la même zone. Une carte ouvre la **page du quiz** (mode, zone, lancement, progression), d'où
« Accueil » ramène à la liste. Les textes s'accordent au quiz (« Nommer la région »,
« 13 régions trouvées », « Quel est cet État ? »).

Les départements et régions d'outre-mer, l'Alaska et Hawaï sont déplacés en **encarts** près de
la carte principale, dans un cadre. La pastille « globe » au coin de chaque cadre ouvre une carte
du monde qui part de la vue d'ensemble et zoome sur ces terres, dessinées à leur vraie place et
sans les nommer ; on peut ensuite la déplacer et zoomer à la molette. Les noms anglais des États
sont acceptés au clavier (« California »). Les numéros de département (« 75 », « 2A », « 971 »)
ne le sont que dans le quiz des numéros, en « Nom → numéro » : ailleurs, taper 30, 31…
suffirait à citer les départements de la zone 30–39.

**Océans** : Pacifique, Atlantique, Indien, Arctique et Austral, avec leurs mers (la
Méditerranée et la Baltique avec l'Atlantique, la mer Rouge avec l'océan Indien, la baie
d'Hudson avec l'Arctique…). Les limites suivent, simplifiées, celles de l'OHI et du World
Factbook : 60° S pour l'Austral, les méridiens du cap Horn et du cap des Aiguilles, la Tasmanie,
l'Indonésie, le détroit de Béring, l'Islande et le détroit de Davis. Chaque océan est un grand
polygone tracé à la main (`data/oceans.toml`) et dessiné **sous** les terres, qui le recouvrent
le long des côtes : pas de contour côtier à stocker, et aucun jour entre la mer et la terre. Les
terres sont celles de l'atlas du monde, reprises à l'exécution ; seul l'Antarctique est embarqué.
La Caspienne, un lac, n'appartient à aucun océan.

**Mers du monde** : les 53 mers de Natural Earth 1:50m, dont les deux morceaux des « mers
intérieures de la côte occidentale de l'Écosse » réunis en une mer des Hébrides, soit 52. La mer
de Scotia, réduite à un éclat dans Natural Earth, est tracée à la main. Elles sont dessinées sous
les terres comme les océans ; golfes, baies, détroits et haute mer restent neutres. Les zones
sont les océans, chaque mer dans celui qui la contient au quiz des océans (un test le vérifie),
avec des régions pour l'Atlantique (Méditerranée et mer Noire, Europe du Nord, Atlantique Ouest
et Sud) et le Pacifique (Asie du Nord-Est, Asie du Sud-Est, Océanie). La Caspienne n'est que
dans « Monde entier ». Chaque zone a son méridien central : la mer de Béring et la mer de Ross,
coupées à l'antiméridien, y sont d'un seul tenant. En voix française, 48 mers sur 52 sont
reconnues (des prononciations approchées comme « mer de lap tev » pallient les noms inconnus du
modèle) ; Tyrrhénienne, Okhotsk, Bellingshausen et Amundsen se tapent.

**Drapeaux du monde** : les 197 pays, reconnus à leur drapeau, dans les mêmes zones. Deux modes :
« Drapeau → pays » (le drapeau s'affiche en grand, sans carte ; on tape ou dit le nom du pays) et
« Drapeau → carte » (le drapeau remplace le nom dans la question ; on clique sur le pays). En fin
de partie sans carte, chaque drapeau s'affiche au-dessus de son pays, les ratés en rouge.
L'exploration des pays du monde montre aussi le drapeau du pays choisi. Ce quiz est dérivé à
l'exécution de l'atlas du monde ; seuls les drapeaux sont stockés (`assets/flags.bin`, 276 Ko).
Ils viennent des SVG de [hampusborgos/country-flags](https://github.com/hampusborgos/country-flags)
(d'après Wikimedia Commons, domaine public), aux proportions officielles (Suisse carrée,
Qatar 28:11, Népal), et sont convertis par `mapgen flags` en **calques de polygones** : courbes
aplaties, traits changés en surfaces, `clipPath` appliqués (croix de Saint-Patrick de l'Union
Jack), dégradés remplacés par leur couleur moyenne, puis coordonnées entières codées par
différences. L'application les peint sur un canvas, nets à toute taille, sans moteur SVG.
`mapgen flag-sheet` produit une planche de contrôle (original rendu par resvg à côté de chaque
drapeau converti).

**Fleuves du monde** : 55 grands fleuves (qui se jettent dans la mer ou une mer fermée ; pas
d'affluents comme le Missouri), par continent : Europe (14), Asie (16), Afrique (8), Amérique du
Nord (8), Amérique du Sud (6), Océanie (3). Les tracés viennent de Natural Earth 1:10m : un fleuve
y est découpé selon ses noms locaux, que `data/rivers.toml` réunit (Yangtsé = Tongtian + Jinsha +
Chang Jiang + Yangtze) en écartant les homonymes (Colorado d'Argentine) ; le Saint-Laurent, presque
absent des données, est tracé à la main. Les fleuves sont dessinés **par-dessus** les terres, en
cyan, avec les 50 plus grands lacs (Natural Earth 1:50m : Grands Lacs, Victoria, Nasser…) ; un
clic ou le survol désigne le fleuve le plus proche, à 8 pixels près. Le fleuve à nommer est
épaissi, avec un halo. Toutes les formes orales sont reconnues en français, dont des
prononciations approchées pour les noms inconnus du modèle (« brahma poutre », « tôt cantine »).

### Ajouter un quiz

Un quiz de subdivisions (provinces, Länder…) se décrit dans un fichier `data/atlases/<id>.toml`
(voir ceux existants) :
- `[atlas]` : titre, catégorie, noms au singulier et au pluriel, genre, pays Natural Earth,
  cadre des terres voisines, et au besoin `tolerance` (simplification des contours) ;
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

Un **minuteur** facultatif s'active sur la page du quiz. Par défaut, il dure 15 minutes pour
citer les 197 pays du monde, au prorata du nombre de pays de la zone, et un peu plus par pays en
« Nommer » (7 s) et « Trouver » (9 s). La durée se règle pour chaque mode et chaque zone. À zéro,
la partie est validée : les pays restants comptent comme ratés.

La **progression** s'affiche sur la page du quiz, sous le lancement : l'évolution du **score** et
du **temps** partie après partie, pour le mode et la zone choisis. Une série regroupe les parties
d'un même quiz, mode et zone : « Tous les pays » sur le monde, sur l'Afrique et sur l'Afrique du
Nord sont trois séries distinctes. Le survol d'un point affiche la date et la valeur. Sous les
courbes, la liste des parties (les cinq dernières, ou toutes à la demande) permet d'écarter celles qui ne comptent pas (fenêtre oubliée, partie
interrompue) : la corbeille demande confirmation dans la ligne même (Échap pour renoncer), puis
un bouton « Annuler » (ou Ctrl+Z) permet encore de revenir en arrière. Les records et les lieux
« à revoir » en tiennent compte.

Chaque partie garde aussi le **détail par lieu** : trouvé du premier coup, après des erreurs ou
raté, le temps de réponse et, en modes « Nommer » et « Localiser », les lieux donnés à tort. On en
tire un **niveau de maîtrise** par lieu, dans un quiz et un mode, toutes zones confondues (la
France jouée dans « Europe » ou dans « Monde », c'est le même savoir) :
- **à revoir** : raté la dernière fois ;
- **acquis** : trouvé du premier coup lors des deux dernières parties ;
- **en cours** : entre les deux.

La carte « À revoir » de la page du quiz compte les lieux de la zone par niveau et liste ceux à
revoir. Le détail vit dans la partie : la supprimer l'efface aussi de la maîtrise. Les parties
enregistrées avant ce détail ne comptent que par leurs lieux ratés. En abandonnant une partie
« Nommer » ou « Localiser », les questions jamais posées ne comptent pas.

Deux façons de s'améliorer s'appuient dessus :
- **Révision** : « Réviser ces N pays » (carte « À revoir », jusqu'à 20 lieux, les plus souvent
  ratés d'abord) ou « Réviser les erreurs » (bilan de fin de partie) lance une partie courte
  limitée à ces lieux, cadrée sur eux, sans minuteur. Elle compte pour la maîtrise mais pas dans
  les séries ni les records ; l'historique de l'accueil la marque « Révision ».
- **Seconde chance** (modes « Nommer » et « Localiser ») : un lieu raté ou passé revient trois
  questions plus tard, affiché comme « à trouver » pour que la carte ne souffle pas la réponse.
  Le score et le bilan du lieu restent ceux d'un raté ; le bilan de fin compte les lieux
  retrouvés. Pas de seconde chance s'il ne reste plus de question après le lieu raté. Activée
  par défaut, elle se désactive sur la page du quiz, à côté du minuteur (réglage enregistré).

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

Un clic sur l'aperçu (ou « Explorer la carte ») ouvre la carte **sans quiz** : tous les lieux de
la zone nommés (numérotés pour les numéros des départements), et leur liste à côté, par
sous-région ; un nom cliqué y emmène la carte. De quoi faire connaissance avant de se tester.

Sur la page du quiz, un **aperçu** montre la zone choisie, cadrée comme en partie, avec ses pays
en bleu. Dès qu'il y a des parties dans le mode choisi, il passe en **carte de maîtrise** : chaque
lieu prend la couleur de son niveau (vert acquis, jaune en cours, orange à revoir, gris jamais
joué), comme la carte « À revoir ». Le choix « Zone / Maîtrise » de l'aperçu permet de revenir à
la zone seule ; l'exploration ouverte depuis l'aperçu reprend la carte de maîtrise, avec sa
légende.

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
- `progress.json` : historique des parties, avec le détail par lieu (JSON compact) ;
- `settings.json` : langue, minuteur (activé ou non, durées personnalisées), seconde chance ;
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

## Linux

L'identifiant de l'application est `com.paul_mesnilgrente.GeoLearner` (domaine
`paul-mesnilgrente.com` inversé). Il nomme le fichier `.desktop`, le metainfo AppStream et
l'icône (`packaging/linux/`), et la fenêtre le déclare (`APP_ID` dans `crates/app/src/main.rs`)
pour que le bureau lui associe son icône.

Chaque tag `v*` déclenche le workflow *Paquets Linux* : compilation sur Ubuntu 22.04 (le binaire
tourne donc avec une glibc ≥ 2.35), puis `scripts/package-linux.sh`, qui joint
`geo-learner-<version>-x86_64-linux.tar.gz` à la release GitHub. Installation dans le dossier
utilisateur :

```sh
tar -xzf geo-learner-<version>-x86_64-linux.tar.gz -C ~/.local --strip-components=1
```

Le binaire arrive dans `~/.local/bin` (qui doit être dans le `PATH`), le lanceur et l'icône dans
`~/.local/share`. Seule dépendance système : `libasound.so.2` (ALSA), présente partout.

Le workflow joint aussi à la release un paquet Flatpak autonome
(`flatpak install --user geo-learner-<version>-x86_64.flatpak`), construit avec le manifeste de
`packaging/flatpak/`. Construction locale et conditions de Flathub : [`docs/flathub.md`](docs/flathub.md). Après chaque changement de `Cargo.lock`, relancer
`python3 scripts/flatpak-cargo-sources.py` pour régénérer les sources Cargo hors ligne.

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
crates/mapgen     génération des atlas (assets/*.bin) depuis Natural Earth (admin-0 et admin-1),
                  des océans et mers (data/oceans.toml, data/seas.toml, `mapgen water`), des
                  fleuves (data/rivers.toml, `mapgen rivers`) et des drapeaux (`mapgen flags`)
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
- `giveup`, `summary` : partie abandonnée, bilan affiché ;
- `progress=14` : page du quiz avec 14 parties fictives dans la zone choisie
  (et quelques autres séries) ; `delete` supprime la dernière, `confirm` affiche sa demande de
  confirmation, `bottom` défile en bas de page ;
- `globe=N` : carte de situation de l'encart n° N ouverte ;
- `explore` : exploration ouverte, avec un lieu choisi (`explore=CA-PE`) ; avec `progress`, elle
  part de l'aperçu, carte de maîtrise comprise ;
- `lang=en` : langue imposée, sans l'enregistrer ;
- `review=SVN,SVK` : partie de révision limitée à ces lieux ;
- `skips=3` : questions passées après les réponses (`target=FRA;answers=DEU,DEU,DEU;skips=3`
  amène la seconde chance de la France) ;
- `home` : accueil affiché, même avec `atlas`, `region` ou `progress` (sinon page du quiz) ;
- `flight=0.5` : zoom de la carte de situation figé à cet instant (0 à 1).

`GEO_WINDOW=1600x900` impose la taille de la fenêtre.

## Choix techniques

- **Interface** : [Iced](https://iced.rs) 0.14, rendu GPU (wgpu), avec repli sur le rendu
  logiciel `tiny-skia`. Le rendu logiciel seul était plus léger d'environ 3 Mo, mais il laissait
  des zones mal redessinées au survol des boutons.
- **Carte** : Natural Earth 1:50m (domaine public), en projection Natural Earth. Elle existe en
  deux niveaux de détail selon le zoom. Les micro-États sont signalés par des pastilles cliquables.
- **Drapeaux** : polygones précalculés (voir « Drapeaux du monde »), sans dépendance SVG dans
  l'application ; `resvg` et `i_overlay` ne servent qu'à `mapgen`.
- **Audio** : `cpal` (ALSA ou PipeWire sous Linux, WASAPI sous Windows).
- **Polices** : Inter (licence OFL, voir `assets/fonts/LICENSE-Inter.txt`).
