#!/usr/bin/env python3
"""Génère l'icône de l'application (globe en projection orthographique) à partir des contours
Natural Earth, puis ses déclinaisons PNG (Microsoft Store, fenêtre).

Usage : python3 scripts/make-icon.py   (nécessite Inkscape et ImageMagick)
"""

import json
import math
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / ".cache" / "ne_50m_admin_0_countries.geojson"
SVG = ROOT / "assets" / "icon.svg"
STORE = ROOT / "packaging" / "windows" / "Assets"

# Point de vue : Europe et Afrique de face.
LON0, LAT0 = 12.0, 28.0
SIZE, CENTER, RADIUS = 512, 256, 188
FOUND = {"FRA", "ESP", "DZA", "EGY", "NGA", "COD", "SAU", "IRN"}  # pays « trouvés », en vert

BACKGROUND = ("#1f2a3b", "#0d1117")
OCEAN = ("#5b98ff", "#2c5fd1")
LAND = "#e3eaf2"
GREEN = "#2fbf71"
BORDER = "#9fb0c4"


def project(lon, lat):
    """Projection orthographique ; un point de la face cachée est ramené sur le bord."""
    l, p = math.radians(lon - LON0), math.radians(lat)
    p0 = math.radians(LAT0)
    x = math.cos(p) * math.sin(l)
    y = math.cos(p0) * math.sin(p) - math.sin(p0) * math.cos(p) * math.cos(l)
    visible = math.sin(p0) * math.sin(p) + math.cos(p0) * math.cos(p) * math.cos(l)
    if visible < 0:
        n = math.hypot(x, y) or 1.0
        x, y = x / n, y / n
    return CENTER + RADIUS * x, CENTER - RADIUS * y, visible >= 0


def rings(geometry):
    coords = geometry["coordinates"]
    polygons = [coords] if geometry["type"] == "Polygon" else coords
    for polygon in polygons:
        yield from polygon


def path_for(feature):
    parts = []
    for ring in rings(feature["geometry"]):
        points = [project(lon, lat) for lon, lat in ring[::2]]
        if not any(v for _, _, v in points):
            continue
        parts.append("M" + "L".join(f"{x:.1f},{y:.1f}" for x, y, _ in points) + "Z")
    return "".join(parts)


def svg():
    features = json.loads(SOURCE.read_text())["features"]
    land, found = [], []
    for f in features:
        code = f["properties"]["ADM0_A3"]
        if code == "ATA":
            continue
        d = path_for(f)
        if d:
            (found if code in FOUND else land).append(d)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {SIZE} {SIZE}">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{BACKGROUND[0]}"/><stop offset="1" stop-color="{BACKGROUND[1]}"/>
    </linearGradient>
    <radialGradient id="ocean" cx="0.36" cy="0.3" r="0.85">
      <stop offset="0" stop-color="{OCEAN[0]}"/><stop offset="1" stop-color="{OCEAN[1]}"/>
    </radialGradient>
    <radialGradient id="shade" cx="0.36" cy="0.3" r="0.9">
      <stop offset="0.55" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity="0.35"/>
    </radialGradient>
    <clipPath id="globe"><circle cx="{CENTER}" cy="{CENTER}" r="{RADIUS}"/></clipPath>
  </defs>
  <rect x="16" y="16" width="{SIZE - 32}" height="{SIZE - 32}" rx="112" fill="url(#bg)"/>
  <circle cx="{CENTER}" cy="{CENTER}" r="{RADIUS}" fill="url(#ocean)"/>
  <g clip-path="url(#globe)" stroke="{BORDER}" stroke-width="0.8" stroke-linejoin="round">
    <path fill="{LAND}" fill-rule="evenodd" d="{''.join(land)}"/>
    <path fill="{GREEN}" fill-rule="evenodd" d="{''.join(found)}"/>
  </g>
  <circle cx="{CENTER}" cy="{CENTER}" r="{RADIUS}" fill="url(#shade)"/>
  <circle cx="{CENTER}" cy="{CENTER}" r="{RADIUS}" fill="none" stroke="#ffffff" stroke-opacity="0.35" stroke-width="5"/>
</svg>
"""


def render_master():
    """Image maître 1024 px. Inkscape gère les dégradés et découpes, contrairement au moteur
    SVG interne d'ImageMagick. Chemins absolus : Inkscape (snap) ne voit pas /tmp."""
    master = ROOT / ".cache" / "icon-1024.png"
    master.parent.mkdir(exist_ok=True)
    subprocess.run(
        ["inkscape", str(SVG), "--export-type=png", f"--export-filename={master}", "-w", "1024"],
        check=True,
        capture_output=True,
    )
    return master


def png(master, size, out, width=None):
    """Réduit l'icône (carrée) ; `width` permet un format large avec l'icône centrée."""
    width = width or size
    args = ["magick", str(master), "-resize", f"{size}x{size}", "-background", "none"]
    if width != size:
        args += ["-gravity", "center", "-extent", f"{width}x{size}"]
    args.append(str(out))
    subprocess.run(args, check=True)


def main():
    SVG.write_text(svg())
    master = render_master()
    STORE.mkdir(parents=True, exist_ok=True)
    # Logos du paquet MSIX (tailles exactes attendues à l'échelle 100 %).
    png(master, 44, STORE / "Square44x44Logo.png")
    png(master, 150, STORE / "Square150x150Logo.png")
    png(master, 150, STORE / "Wide310x150Logo.png", width=310)
    png(master, 50, STORE / "StoreLogo.png")
    png(master, 512, ROOT / "assets" / "icon-512.png")
    # Icône de la fenêtre : RGBA brut 64×64, chargé tel quel par l'application.
    subprocess.run(
        ["magick", str(master), "-resize", "64x64", "-depth", "8", f"rgba:{ROOT / 'assets' / 'icon-64.rgba'}"],
        check=True,
    )
    print("icônes générées")


if __name__ == "__main__":
    main()
