#!/usr/bin/env sh
# Régénère les atlas (assets/*.bin) à partir de Natural Earth et des fichiers de data/ :
# pays du monde (admin-0, 1:50m), océans (data/oceans.toml), mers (data/seas.toml, polygones
# marins 1:50m) et subdivisions de data/atlases/ (admin-1, 1:10m).
set -eu
cd "$(dirname "$0")/.."

NE=https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson
ADMIN0=.cache/ne_50m_admin_0_countries.geojson
ADMIN1=.cache/ne_10m_admin_1.geojson
MARINE=.cache/ne_50m_geography_marine_polys.geojson
mkdir -p .cache
[ -f "$ADMIN0" ] || curl -fL -o "$ADMIN0" "$NE/ne_50m_admin_0_countries.geojson"
[ -f "$ADMIN1" ] || curl -fL -o "$ADMIN1" "$NE/ne_10m_admin_1_states_provinces.geojson"
[ -f "$MARINE" ] || curl -fL -o "$MARINE" "$NE/ne_50m_geography_marine_polys.geojson"

cargo run --release -p mapgen -- world "$ADMIN0" assets/world.bin
cargo run --release -p mapgen -- water data/oceans.toml "$ADMIN0" "$MARINE" assets/oceans.bin
cargo run --release -p mapgen -- water data/seas.toml "$ADMIN0" "$MARINE" assets/seas.bin
for atlas in data/atlases/*.toml; do
    name=$(basename "$atlas" .toml)
    cargo run --release -p mapgen -- atlas "$atlas" "$ADMIN0" "$ADMIN1" "assets/$name.bin"
done
