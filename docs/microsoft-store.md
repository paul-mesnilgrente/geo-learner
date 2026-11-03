# Publier GeoLearner sur le Microsoft Store

Ce qui est prêt dans le dépôt :

| Élément | Emplacement |
|---|---|
| Manifeste du paquet MSIX | `packaging/windows/AppxManifest.xml` |
| Identité du paquet (à compléter) | `packaging/windows/store.json` |
| Logos du paquet | `packaging/windows/Assets/` (générés par `scripts/make-icon.py`) |
| Construction du paquet | `scripts/package-msix.ps1`, ou le workflow GitHub « Paquet Microsoft Store » |
| Politique de confidentialité | `PRIVACY.md` (adresse de contact à compléter) |
| Captures d'écran (1600×900) et logo 300×300 | `docs/store/` |

L'exécutable intègre le runtime Visual C++ (`crt-static`) : aucune dépendance à installer.

## 1. Compte développeur

Créer un compte sur [Partner Center](https://storedeveloper.microsoft.com) (compte individuel,
gratuit pour les particuliers). La vérification d'identité peut prendre quelques jours.

## 2. Réserver le nom

*Applications et jeux* → *Nouveau produit* → *Application MSIX ou PWA* → réserver
**GeoLearner** (ou un autre nom s'il est pris).

## 3. Reporter l'identité du paquet

Dans *Gestion des produits* → *Identité du produit*, copier dans `packaging/windows/store.json` :

| Partner Center | `store.json` |
|---|---|
| `Package/Identity/Name` | `identityName` |
| `Package/Identity/Publisher` (`CN=…`) | `publisher` |
| `Package/Properties/PublisherDisplayName` | `publisherDisplayName` |

Le nom affiché (`displayName`) doit être le nom réservé.

## 4. Publier la politique de confidentialité

Le Store exige une **adresse publique** (le micro et l'accès Internet sont déclarés). Le dépôt
étant privé, au choix :

- un [gist public](https://gist.github.com) contenant `PRIVACY.md` ;
- rendre le dépôt public et pointer vers `PRIVACY.md` ;
- tout autre hébergement de page web.

Publiée dans un [gist public](https://gist.github.com/paul-mesnilgrente/5884474ce8f49255bbf82e746cb33adc)
(fichier `GeoLearner_PRIVACY.md`). Toute modification de `PRIVACY.md` doit y être reportée.

Adresse de contact : big.learner.app@gmail.com.

## 5. Construire le paquet

- Version : modifier `version` dans `Cargo.toml` (section `[workspace.package]`) ; le paquet
  prend la version `x.y.z.0`. Chaque soumission doit avoir une version plus élevée que la
  précédente.
- Pousser un tag (`git tag v0.1.0 && git push --tags`) ou lancer le workflow
  *Paquet Microsoft Store* depuis l'onglet *Actions* ; récupérer l'artefact `geo-learner-msix`.
- Sous Windows, on peut aussi lancer `cargo build --release -p geo-learner` puis
  `pwsh scripts/package-msix.ps1`.

Le paquet n'est pas signé : le Store le signe lui-même.

**Essai local avant soumission** (Windows, mode développeur activé) : renommer le `.msix` en
`.zip`, l'extraire, puis `Add-AppxPackage -Register <dossier>\AppxManifest.xml`. L'application
apparaît alors dans le menu Démarrer comme si elle venait du Store. Le *Windows App
Certification Kit* (fourni avec le SDK Windows) permet aussi de lancer les tests de certification.

## 6. Soumission

| Section | Contenu |
|---|---|
| Tarification | Gratuit (prix 0 $), tous les marchés |
| Propriétés | Catégorie **Éducation** ; politique de confidentialité : <https://gist.github.com/paul-mesnilgrente/5884474ce8f49255bbf82e746cb33adc> |
| Classification par âge | Questionnaire IARC : aucun contenu sensible → PEGI 3 attendu |
| Paquets | Le `.msix` |
| Fiches (français et anglais : le paquet déclare les deux langues, chacune exige sa fiche) | Textes ci-dessous, captures `docs/store/fr/` et `docs/store/en/` (dans l'ordre), logo `docs/store/logo-300.png` |
| Remarques pour la certification | Texte ci-dessous (la capacité `runFullTrust` doit être justifiée) |

Les deux fiches ont les mêmes champs. Partner Center limite la description courte à environ
270 caractères recommandés et les mots-clés à 7.

### Nouveautés de la version 0.2.0 (« What's new in this version »)

> Nouveautés de la version 0.2 :
> • Interface en anglais : GeoLearner suit la langue du système, modifiable sur l'accueil. La
> saisie vocale existe aussi en anglais.
> • Trois nouveaux quiz : provinces et territoires du Canada, États et territoires d'Australie,
> numéros des départements (sans carte, dans les deux sens).
> • Zones « Par numéro » pour apprendre les départements par dizaine.
> • Nouvel accueil en cartes ; chaque quiz a sa page, avec sa progression.
> • Explorer la carte librement, sans quiz, avec le nom de chaque lieu.
> • Maîtrise de chaque lieu (acquis, en cours, à revoir), carte de maîtrise et parties de
> révision.
> • Seconde chance en modes Nommer et Trouver : un lieu raté revient quelques questions plus
> tard (désactivable).
> • Carte de situation des encarts (outre-mer, Alaska, Hawaï).

> What's new in version 0.2:
> • English interface: GeoLearner follows the system language, which you can change on the home
> screen. Voice input is available in English too.
> • Three new quizzes: provinces and territories of Canada, states and territories of Australia,
> and French department numbers (no map, both ways).
> • "By number" zones to learn the French departments ten at a time.
> • New home screen with quiz cards; each quiz has its own page with your progress.
> • Explore the map freely, without a quiz, with every place named.
> • Mastery of each place (learned, in progress, to review), mastery map and review games.
> • Second chance in Name and Find modes: a missed place comes back a few questions later
> (can be turned off).
> • Locator map for the insets (French overseas departments, Alaska, Hawaii).

### Description courte

> Apprenez les pays du monde, les départements français, les États américains, les provinces
> canadiennes et les États australiens sur une carte interactive, au clavier ou à la voix.

> Learn the countries of the world, U.S. states, Canadian provinces, Australian states and French
> departments on an interactive map, by typing or by voice.

### Description

> GeoLearner vous aide à retenir la carte du monde, et bien plus : les 197 pays, les
> départements et les régions de France, les 50 États des États-Unis, les provinces et
> territoires du Canada, les États et territoires d'Australie.
>
> Trois façons de jouer :
> • Tout citer : nommez tous les lieux d'une zone, dans l'ordre que vous voulez. Chaque réponse
> juste se colore sur la carte.
> • Nommer : un lieu s'allume, à vous de trouver son nom.
> • Trouver sur la carte : un nom s'affiche, cliquez au bon endroit.
>
> Jouez sur le monde entier, un continent, une région (Balkans, Caraïbes, Asie centrale,
> Polynésie…) ou un regroupement comme les Petites Antilles ou l'Union européenne. Côté France,
> révisez la métropole, l'outre-mer, une seule région ou les départements par dizaine, et
> apprenez leurs numéros dans les deux sens.
>
> GeoLearner retient ce que vous savez : chaque lieu est acquis, en cours ou à revoir, et une
> partie de révision reprend ceux que vous ratez. La seconde chance fait revenir un lieu raté
> quelques questions plus tard. Explorez aussi la carte librement, sans quiz, pour faire
> connaissance avant de vous tester.
>
> Saisie vocale : énumérez les noms à voix haute, ils sont validés au fil de la parole. La
> reconnaissance se fait entièrement sur votre ordinateur, sans envoyer votre voix sur Internet.
>
> Minuteur facultatif, réglable pour chaque mode et chaque zone. Chaque quiz trace l'évolution
> de votre score et de votre temps, partie après partie.
>
> Interface en français et en anglais. Tout reste sur votre ordinateur. Aucun compte, aucune
> publicité.

> GeoLearner helps you learn the world map and more: all 197 countries, the 50 U.S. states, the
> provinces and territories of Canada, the states and territories of Australia, and the regions
> and departments of France.
>
> Three ways to play:
> • Name them all: list every place in an area, in any order. Each correct answer lights up on
> the map.
> • Name it: a place is highlighted, you give its name.
> • Find it on the map: a name is shown, you click the right place.
>
> Play the whole world, a continent, a subregion (Balkans, Caribbean, Central Asia,
> Polynesia…) or a group such as the Lesser Antilles or the European Union. For the U.S., play
> a region or a census division.
>
> GeoLearner keeps track of what you know: each place is learned, in progress or to review, and
> a review game brings back the ones you miss. With second chance, a missed place comes back a
> few questions later. You can also explore the map freely, without a quiz, before testing
> yourself.
>
> Voice input: say the names out loud and they are accepted as you speak. Speech recognition
> runs entirely on your computer; your voice is never sent over the Internet.
>
> Optional timer, adjustable for each mode and area. Each quiz charts your score and time, game
> after game.
>
> English and French interface. Everything stays on your computer. No account, no ads.

### Fonctionnalités (liste du Store)

- 197 pays : les 193 membres de l'ONU, plus le Vatican, la Palestine, le Kosovo et Taïwan
- Départements (101) et régions (18) de France, États-Unis (50), Canada (13), Australie (8)
- Numéros des départements, dans les deux sens et par dizaine
- Trois modes de jeu : tout citer, nommer, trouver sur la carte
- Monde, continents, sous-régions et regroupements d'îles
- Maîtrise de chaque lieu, carte « À revoir » et parties de révision
- Seconde chance et minuteur facultatifs
- Saisie vocale hors ligne, en français et en anglais
- Courbes de progression : score et temps, partie après partie
- Carte à explorer librement, sans quiz
- Interface en français et en anglais

- 197 countries: the 193 UN members plus Vatican City, Palestine, Kosovo and Taiwan
- U.S. states (50), Canada (13), Australia (8), French departments (101) and regions (18)
- French department numbers, both ways and ten at a time
- Three game modes: name them all, name it, find it on the map
- World, continents, subregions and island groups
- Mastery of each place, review list and review games
- Optional second chance and timer
- Offline voice input in English and French
- Progress charts: score and time, game after game
- Explore the map freely, without a quiz
- English and French interface

### Mots-clés

Le Store en accepte 7 au plus.
- Français : géographie, pays, carte du monde, quiz, départements, États-Unis, éducation.
- Anglais : geography, countries, world map, quiz, US states, Canada provinces, education.

### Captures

Dans l'ordre (1600×900, générées par le mode démo avec `lang=fr` ou `lang=en`, voir le
README), dans `docs/store/fr/` et `docs/store/en/` :

1. Accueil : les quiz en cartes et les dernières parties
2. Pays du monde, « Tous les pays » en cours
3. Page du quiz : modes, zones, aperçu de maîtrise, progression
4. Carte de maîtrise (exploration de l'Europe)
5. Pays du monde, « Nommer le pays »
6. Départements français, avec les encarts de l'outre-mer
7. Numéros des départements, « Numéro → nom »
8. Exploration du Canada
9. Bilan d'une partie, avec « Réviser les erreurs »

### Remarques pour la certification

> Application de bureau Win32 écrite en Rust et empaquetée en MSIX, d'où la capacité
> runFullTrust. Le micro sert à la saisie vocale facultative, traitée localement. L'accès
> Internet sert uniquement au téléchargement, à la demande de l'utilisateur, du modèle de
> reconnaissance vocale de la langue choisie (environ 42 Mo, alphacephei.com), vérifié par empreinte SHA-256.
> L'application fonctionne entièrement sans micro ni Internet (saisie au clavier).

## 7. Mises à jour

1. Augmenter `version` dans `Cargo.toml` (le Store refuse une version égale ou inférieure ; le
   4ᵉ chiffre du paquet reste 0, il est réservé au Store).
2. Pousser le tag `vX.Y.Z` : le workflow construit le `.msix`.
3. Partner Center → *Update* : la soumission reprend les réponses précédentes. Remplacer le
   paquet, remplir *What's new in this version* dans chaque fiche, changer les captures si
   l'interface a changé, puis *Submit for certification*.
