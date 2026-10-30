#!/usr/bin/env sh
# Régénère assets/world.bin à partir des contours Natural Earth (1:50m) et des fichiers de data/.
set -eu
cd "$(dirname "$0")/.."

SRC=.cache/ne_50m_admin_0_countries.geojson
if [ ! -f "$SRC" ]; then
    mkdir -p .cache
    curl -fL -o "$SRC" \
        https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_50m_admin_0_countries.geojson
fi
cargo run --release -p mapgen -- "$SRC" assets/world.bin
