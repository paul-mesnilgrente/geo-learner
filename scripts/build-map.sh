#!/usr/bin/env sh
# Régénère les atlas (assets/*.bin) à partir de Natural Earth et des fichiers de data/ :
# pays du monde (admin-0, 1:50m), océans (data/oceans.toml), mers (data/seas.toml, polygones
# marins 1:50m), fleuves (data/rivers.toml, tracés 1:10m et lacs 1:50m), lacs (data/lakes.toml,
# 1:50m), capitales (data/capitals.toml, lieux habités 1:50m), préfectures et capitales des
# États (data/points/, positions Wikidata), drapeaux (SVG de
# hampusborgos/country-flags, domaine public) et subdivisions de data/atlases/ (admin-1, 1:10m).
set -eu
cd "$(dirname "$0")/.."

NE=https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson
ADMIN0=.cache/ne_50m_admin_0_countries.geojson
ADMIN1=.cache/ne_10m_admin_1.geojson
MARINE=.cache/ne_50m_geography_marine_polys.geojson
RIVERS=.cache/ne_10m_rivers_lake_centerlines.geojson
LAKES=.cache/ne_50m_lakes.geojson
PLACES=.cache/ne_50m_populated_places.geojson
FLAGS=.cache/country-flags/country-flags-main/svg
mkdir -p .cache
[ -f "$ADMIN0" ] || curl -fL -o "$ADMIN0" "$NE/ne_50m_admin_0_countries.geojson"
[ -f "$ADMIN1" ] || curl -fL -o "$ADMIN1" "$NE/ne_10m_admin_1_states_provinces.geojson"
[ -f "$MARINE" ] || curl -fL -o "$MARINE" "$NE/ne_50m_geography_marine_polys.geojson"
[ -f "$RIVERS" ] || curl -fL -o "$RIVERS" "$NE/ne_10m_rivers_lake_centerlines.geojson"
[ -f "$LAKES" ] || curl -fL -o "$LAKES" "$NE/ne_50m_lakes.geojson"
[ -f "$PLACES" ] || curl -fL -o "$PLACES" "$NE/ne_50m_populated_places.geojson"
if [ ! -d "$FLAGS" ]; then
    mkdir -p .cache/country-flags
    curl -fL https://codeload.github.com/hampusborgos/country-flags/tar.gz/refs/heads/main \
        | tar xz -C .cache/country-flags
fi

cargo run --release -p mapgen -- world "$ADMIN0" assets/world.bin
cargo run --release -p mapgen -- water data/oceans.toml "$ADMIN0" "$MARINE" assets/oceans.bin
cargo run --release -p mapgen -- water data/seas.toml "$ADMIN0" "$MARINE" assets/seas.bin
cargo run --release -p mapgen -- rivers data/rivers.toml "$RIVERS" "$LAKES" assets/rivers.bin
cargo run --release -p mapgen -- lakes data/lakes.toml "$LAKES" assets/lakes.bin
cargo run --release -p mapgen -- capitals data/capitals.toml "$PLACES" assets/world.bin assets/capitals.bin
cargo run --release -p mapgen -- flags "$ADMIN0" "$FLAGS" assets/world.bin assets/flags.bin
for atlas in data/atlases/*.toml; do
    name=$(basename "$atlas" .toml)
    cargo run --release -p mapgen -- atlas "$atlas" "$ADMIN0" "$ADMIN1" "assets/$name.bin"
done
# Points posés sur un atlas de subdivisions (préfectures, capitales des États).
cargo run --release -p mapgen -- points data/points/fr-prefectures.toml \
    data/atlases/fr-departements.toml "$ADMIN0" "$ADMIN1" assets/fr-prefectures.bin
cargo run --release -p mapgen -- points data/points/us-capitales.toml \
    data/atlases/us-etats.toml "$ADMIN0" "$ADMIN1" assets/us-capitales.bin
