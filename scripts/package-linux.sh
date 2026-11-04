#!/bin/sh
# Construit l'archive Linux à partir du binaire release déjà compilé :
#   target/linux/geo-learner-<version>-x86_64-linux.tar.gz
#
# L'archive reprend l'arborescence d'un préfixe d'installation (bin/, share/) :
#   tar -xzf geo-learner-<version>-x86_64-linux.tar.gz -C ~/.local --strip-components=1
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
ID=com.paul_mesnilgrente.GeoLearner
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)
NAME="geo-learner-$VERSION-$(uname -m)-linux"
OUT="$ROOT/target/linux"
DIR="$OUT/$NAME"

rm -rf "$DIR"
install -Dm755 "$ROOT/target/release/geo-learner" "$DIR/bin/geo-learner"
install -Dm644 "$ROOT/packaging/linux/$ID.desktop" "$DIR/share/applications/$ID.desktop"
install -Dm644 "$ROOT/packaging/linux/$ID.metainfo.xml" "$DIR/share/metainfo/$ID.metainfo.xml"
install -Dm644 "$ROOT/assets/icon.svg" "$DIR/share/icons/hicolor/scalable/apps/$ID.svg"
install -Dm644 "$ROOT/assets/icon-512.png" "$DIR/share/icons/hicolor/512x512/apps/$ID.png"
install -Dm644 "$ROOT/LICENSE" "$DIR/share/licenses/geo-learner/LICENSE"
install -Dm644 "$ROOT/assets/fonts/LICENSE-Inter.txt" "$DIR/share/licenses/geo-learner/LICENSE-Inter.txt"

tar -czf "$OUT/$NAME.tar.gz" -C "$OUT" "$NAME"
echo "$OUT/$NAME.tar.gz"
