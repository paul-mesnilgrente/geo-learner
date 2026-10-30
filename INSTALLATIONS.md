# Ce qui a été installé sur la machine

Tout a été installé dans le dossier utilisateur : rien n'a été installé avec `sudo` ou `apt`.

| Élément | Emplacement | Taille approx. | Pour le désinstaller |
|---|---|---|---|
| Toolchain Rust via **rustup** (rustc 1.99, cargo, clippy, rustfmt) | `~/.rustup`, `~/.cargo/bin` | ~310 Mo | `rustup self uninstall` |
| Cache des crates téléchargées (iced, cpal, utter…) | `~/.cargo/registry` | ~350 Mo | supprimé avec rustup, ou `rm -rf ~/.cargo/registry` |
| Modèle vocal Vosk français (téléchargé par l'application) | `~/.local/share/geo-learner/models/` | ~65 Mo | `rm -rf ~/.local/share/geo-learner` |

rustup a été installé **sans modifier le PATH**. Pour utiliser `cargo`, ajoute ceci à `~/.zshrc` :

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

Le `rustc` 1.93 déjà présent dans `/usr/bin` (paquet Ubuntu) n'a pas été touché.

## Dans le dépôt (ignorés par git)

| Élément | Emplacement | Pour le supprimer |
|---|---|---|
| Artefacts de compilation | `target/` (~2,5 Go) | `cargo clean` |
| Contours Natural Earth sources | `.cache/` (3 Mo) | `rm -rf .cache` |

## À installer toi-même (non fait, nécessite sudo)

```sh
sudo apt install libasound2-dev
```

Ce paquet fournit les en-têtes ALSA nécessaires pour compiler la capture micro (`cpal`).
Pendant cette session, je m'en suis passé avec un fichier `pkg-config` temporaire pointant
vers `libasound.so.2` (dossier temporaire de la session, effacé automatiquement).
Sans ce paquet, `cargo build` échouera sur `alsa-sys`.
