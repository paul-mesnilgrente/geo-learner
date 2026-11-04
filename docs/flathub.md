# GeoLearner en Flatpak (et Flathub)

Ce qui est prêt dans le dépôt :

| Élément | Emplacement |
|---|---|
| Manifeste Flatpak (source locale, pour les essais) | `packaging/flatpak/com.paul_mesnilgrente.GeoLearner.yml` |
| Sources Cargo hors ligne | `packaging/flatpak/cargo-sources.json` (généré par `scripts/flatpak-cargo-sources.py`) |
| Lanceur, fiche AppStream | `packaging/linux/` |
| Icône | `assets/icon.svg`, `assets/icon-512.png` |

Identifiant : `com.paul_mesnilgrente.GeoLearner` (domaine `paul-mesnilgrente.com` inversé, le
tiret devient un tiret bas). Il ne peut plus changer une fois publié.

## 1. Outils (une fois)

```sh
sudo apt install flatpak
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.flatpak.Builder
```

## 2. Essai local

Depuis la racine du dépôt :

```sh
python3 scripts/flatpak-cargo-sources.py          # si Cargo.lock a changé
flatpak run org.flatpak.Builder --user --install --force-clean --disable-rofiles-fuse --install-deps-from=flathub \
  target/flatpak packaging/flatpak/com.paul_mesnilgrente.GeoLearner.yml
flatpak run com.paul_mesnilgrente.GeoLearner
```

Contrôles de Flathub (les mêmes que ceux de la relecture) :

```sh
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest packaging/flatpak/com.paul_mesnilgrente.GeoLearner.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder appstream packaging/linux/com.paul_mesnilgrente.GeoLearner.metainfo.xml
```

Dans la sandbox, les données vont dans `~/.var/app/com.paul_mesnilgrente.GeoLearner/data/geo-learner/`.

## 3. Soumission : à faire soi-même

La [politique de Flathub sur l'IA générative](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy)
s'applique à GeoLearner :

- le manifeste soumis à Flathub ne doit contenir **aucun contenu écrit ou assisté par une IA**,
  même déclaré : celui de `packaging/flatpak/` ne peut pas être envoyé tel quel, il faut écrire
  le sien ;
- la PR de soumission (description, commits, réponses aux relecteurs) doit être faite
  **sans IA**, et il est interdit de demander une relecture par un agent IA ;
- le code, la documentation et le packaging générés par IA doivent être **déclarés** (parties
  concernées, ampleur) ; les relecteurs peuvent refuser selon cette ampleur ;
- les applis à l'historique très court sont généralement refusées, et les traductions
  automatiques ne sont pas acceptées.

La marche à suivre (fork de `flathub/flathub`, branche `new-pr`, liste à cocher) est décrite dans
le [guide de soumission](https://docs.flathub.org/docs/for-app-authors/submission).

En attendant, chaque release GitHub contient un paquet `.flatpak` construit avec le manifeste du
dépôt (workflow *Paquets Linux*) :

```sh
flatpak install --user geo-learner-<version>-x86_64.flatpak
```

Le runtime freedesktop est téléchargé depuis Flathub au besoin. Ce paquet ne se met pas à jour
tout seul : il faut installer le fichier de la nouvelle version.

## 4. Vérification du domaine

Sur [flathub.org](https://flathub.org), se connecter avec GitHub → *Developer portal* →
l'application → *Verification* → site web. Flathub fournit un jeton à publier à
`https://paul-mesnilgrente.com/.well-known/org.flathub.VerifiedApps.txt`.

Le site est servi par GitHub Pages (dépôt `paul-mesnilgrente.github.io`, Jekyll) : Jekyll ignore
les dossiers qui commencent par un point. Il faut donc, dans ce dépôt :

- ajouter le fichier `.well-known/org.flathub.VerifiedApps.txt` contenant le jeton ;
- ajouter `include: [".well-known"]` à `_config.yml` (surtout pas de `.nojekyll`, qui
  désactiverait Jekyll pour tout le site).
