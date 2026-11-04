#!/usr/bin/env python3
"""Génère packaging/flatpak/cargo-sources.json à partir de Cargo.lock.

Flathub compile sans réseau : chaque crate est déclarée comme source (archive de crates.io et
empreinte SHA-256 du Cargo.lock), puis rangée dans un dossier vendor que Cargo utilise à la place
de crates.io. Même format que flatpak-cargo-generator (flatpak-builder-tools), limité aux crates
de crates.io, les seules du projet.

Usage : python3 scripts/flatpak-cargo-sources.py   (à relancer après chaque changement de Cargo.lock)
"""

import json
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "Cargo.lock"
OUT = ROOT / "packaging" / "flatpak" / "cargo-sources.json"
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
VENDOR = "cargo/vendor"

CONFIG = f"""[source.vendored-sources]
directory = "{VENDOR}"

[source.crates-io]
replace-with = "vendored-sources"
"""


def main():
    packages = tomllib.loads(LOCK.read_text())["package"]
    sources = []
    for p in packages:
        source = p.get("source")
        if source is None:
            continue  # crate du workspace
        if source != CRATES_IO:
            raise SystemExit(f"source non prise en charge pour {p['name']} : {source}")
        name, version, checksum = p["name"], p["version"], p["checksum"]
        dest = f"{VENDOR}/{name}-{version}"
        sources.append(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": dest,
            }
        )
        sources.append(
            {
                "type": "inline",
                "contents": json.dumps({"package": checksum, "files": {}}),
                "dest": dest,
                "dest-filename": ".cargo-checksum.json",
            }
        )
    sources.append({"type": "inline", "contents": CONFIG, "dest": "cargo", "dest-filename": "config.toml"})
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(sources, indent=4) + "\n")
    print(f"{OUT.relative_to(ROOT)} : {len(sources) // 2} crates")


if __name__ == "__main__":
    main()
