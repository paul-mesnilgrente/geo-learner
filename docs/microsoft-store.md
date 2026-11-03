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
| Tarification | Gratuit ; marchés au choix (francophones au minimum, l'interface étant en français) |
| Propriétés | Catégorie **Éducation** ; politique de confidentialité : <https://gist.github.com/paul-mesnilgrente/5884474ce8f49255bbf82e746cb33adc> |
| Classification par âge | Questionnaire IARC : aucun contenu sensible → PEGI 3 attendu |
| Paquets | Le `.msix` |
| Fiche (français) | Textes ci-dessous, captures `docs/store/1-…7-*.png` (dans l'ordre), logo `docs/store/logo-300.png` |
| Remarques pour la certification | Texte ci-dessous (la capacité `runFullTrust` doit être justifiée) |

### Description courte

> Apprenez les pays du monde, les départements français et les États américains sur une carte
> interactive, au clavier ou à la voix.

### Description

> GeoLearner vous aide à retenir la carte du monde, et bien plus : les 197 pays, les
> départements et les régions de France, les 50 États des États-Unis.
>
> Trois façons de jouer :
> • Tout citer : nommez tous les lieux d'une zone, dans l'ordre que vous voulez. Chaque réponse
> juste se colore sur la carte.
> • Nommer : un pays ou un département s'allume, à vous de trouver son nom.
> • Trouver sur la carte : un nom s'affiche, cliquez au bon endroit.
>
> Jouez sur le monde entier, un continent, une région (Balkans, Caraïbes, Asie centrale,
> Polynésie…) ou un regroupement comme les Petites Antilles ou l'Union européenne. Côté France,
> révisez la métropole, l'outre-mer ou une seule région ; côté États-Unis, une région ou une
> division du recensement. Les numéros de département sont acceptés (« 75 », « 2A »).
>
> Saisie vocale : énumérez les noms à voix haute, ils sont validés au fil de la parole. La
> reconnaissance se fait entièrement sur votre ordinateur, sans envoyer votre voix sur Internet.
>
> Minuteur facultatif, réglable pour chaque mode et chaque zone. Chaque quiz trace
> l'évolution de votre score et de votre temps, partie après partie, et liste les lieux que vous
> ratez le plus souvent.
>
> Tout reste sur votre ordinateur. Aucun compte, aucune publicité.

### Fonctionnalités (liste du Store)

- 197 pays : les 193 membres de l'ONU, plus le Vatican, la Palestine, le Kosovo et Taïwan
- Départements français (101), régions françaises (18) et États des États-Unis (50)
- Trois modes de jeu : tout citer, nommer, trouver sur la carte
- Monde, continents, sous-régions et regroupements d'îles
- Saisie vocale hors ligne, validée au fil de la parole
- Minuteur facultatif, réglable pour chaque mode et chaque zone
- Courbes de progression : score, temps et lieux à revoir
- Carte interactive : zoom, déplacement, archipels et micro-États repérables

### Mots-clés

Le Store en accepte 7 au plus : géographie, pays, carte du monde, quiz, départements,
États-Unis, éducation.

### Captures

Dans l'ordre (1600×900, générées par le mode démo, voir le README) :

1. Accueil : catalogue des quiz, modes et zones
2. Pays du monde, « Tous les pays » en cours
3. Pays du monde, « Nommer le pays »
4. Départements français, avec les encarts de l'outre-mer
5. États des États-Unis, « Trouver sur la carte »
6. Onglet Progression
7. Bilan d'une partie

### Remarques pour la certification

> Application de bureau Win32 écrite en Rust et empaquetée en MSIX, d'où la capacité
> runFullTrust. Le micro sert à la saisie vocale facultative, traitée localement. L'accès
> Internet sert uniquement au téléchargement, à la demande de l'utilisateur, du modèle de
> reconnaissance vocale (42 Mo, alphacephei.com), vérifié par empreinte SHA-256.
> L'application fonctionne entièrement sans micro ni Internet (saisie au clavier).
