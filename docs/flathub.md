# Publier GeoLearner sur Flathub

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

## 3. Soumission

1. Pousser un tag de version (`vX.Y.Z`) contenant le manifeste et `cargo-sources.json` à jour.
2. Forker [flathub/flathub](https://github.com/flathub/flathub) et créer une branche **à partir
   de la branche `new-pr`**.
3. À la racine de cette branche, ajouter `com.paul_mesnilgrente.GeoLearner.yml` (copie du
   manifeste où la source `dir` est remplacée par le tag) et `cargo-sources.json` :

   ```yaml
       sources:
         - type: git
           url: https://github.com/paul-mesnilgrente/geo-learner.git
           tag: vX.Y.Z
           commit: <hash complet du commit du tag>
         - cargo-sources.json
   ```

4. Ouvrir la PR vers la branche `new-pr` de `flathub/flathub`, titre
   « Add com.paul_mesnilgrente.GeoLearner ». Le modèle de PR contient une liste à cocher.
5. Répondre aux relecteurs. Commenter `bot, build` relance une compilation d'essai.
   Permission à justifier : `--share=network`, uniquement pour télécharger le modèle vocal Vosk
   à la demande de l'utilisateur ; la reconnaissance elle-même est locale.

Une fois la PR fusionnée, Flathub crée le dépôt `flathub/com.paul_mesnilgrente.GeoLearner` et
t'en donne l'accès : les mises à jour se font ensuite par PR sur ce dépôt (nouveau tag, nouveau
commit, `cargo-sources.json` régénéré).

## 4. Vérification du domaine

Sur [flathub.org](https://flathub.org), se connecter avec GitHub → *Developer portal* →
l'application → *Verification* → site web. Flathub fournit un jeton à publier à
`https://paul-mesnilgrente.com/.well-known/org.flathub.VerifiedApps.txt`.

Le site est servi par GitHub Pages (dépôt `paul-mesnilgrente.github.io`, Jekyll) : Jekyll ignore
les dossiers qui commencent par un point. Il faut donc, dans ce dépôt :

- ajouter le fichier `.well-known/org.flathub.VerifiedApps.txt` contenant le jeton ;
- ajouter `include: [".well-known"]` à `_config.yml` (surtout pas de `.nojekyll`, qui
  désactiverait Jekyll pour tout le site).
