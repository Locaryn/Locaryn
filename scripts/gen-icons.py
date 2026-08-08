#!/usr/bin/env python3
"""Génère le jeu d'icônes des applications Locaryn.

Les icônes livrées jusqu'ici étaient des aplats de 32x32 en RVB sans canal
alpha — 99 octets. Windows ne s'en apercevait pas (il utilise le `.ico`), mais
`tauri::generate_context!` refuse un PNG non RVBA : la compilation cassait sur
Linux et macOS, et seulement là.

Le dessin suit la direction visuelle du produit : quasi monochrome, un seul
vert atténué, aucune fioriture. Pas de dégradé, pas de halo.

Reproductible et versionné : régénérer les icônes ne doit pas dépendre d'un
script qu'on supprime après usage.

    python scripts/gen-icons.py
"""

from __future__ import annotations

import pathlib
import sys

try:
    from PIL import Image, ImageDraw
except ImportError:
    sys.exit("Pillow requis :  pip install Pillow")

# Jetons repris de packages-ui/tokens/tokens.css — une icône qui dérive de
# l'interface se remarque immédiatement dans une barre des tâches.
BG = (23, 25, 26, 255)  # --bg      #17191a
ACCENT = (111, 156, 127, 255)  # --accent  #6f9c7f

ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGETS = [
    ROOT / "apps" / "desktop" / "src-tauri" / "icons",
    ROOT / "apps" / "mobile" / "src-tauri" / "icons",
]

# Rendu 8x puis réduction : le seul anticrénelage dont on ait besoin, et il
# évite de dépendre d'un moteur de dessin vectoriel.
SUPER = 8
BASE = 1024


def draw_mark(size: int) -> Image.Image:
    """Le « L » de Locaryn, en deux traits. Un monogramme se lit à 16 px ;
    un logotype non."""
    s = size * SUPER
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Tuile arrondie. Le rayon suit la taille pour que la forme reste la même
    # à toutes les échelles.
    d.rounded_rectangle([0, 0, s - 1, s - 1], radius=int(s * 0.22), fill=BG)

    # Le glyphe : une hampe verticale et un pied horizontal, extrémités
    # arrondies pour éviter l'angle dur qui jure avec la tuile.
    pad = s * 0.30
    thick = s * 0.115
    top = pad
    bottom = s - pad
    left = pad
    right = s - pad * 0.95

    d.rounded_rectangle(
        [left, top, left + thick, bottom],
        radius=thick / 2,
        fill=ACCENT,
    )
    d.rounded_rectangle(
        [left, bottom - thick, right, bottom],
        radius=thick / 2,
        fill=ACCENT,
    )

    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    master = draw_mark(BASE)
    if master.mode != "RGBA":  # ceinture et bretelles : c'est tout le sujet
        master = master.convert("RGBA")

    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    png_sizes = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }

    for out in TARGETS:
        out.mkdir(parents=True, exist_ok=True)

        for name, px in png_sizes.items():
            img = master.resize((px, px), Image.LANCZOS)
            img.save(out / name, "PNG")

        # Un .ico multi-résolutions : Windows pioche la taille qu'il lui faut
        # plutôt que de rééchantillonner une image unique.
        master.resize((256, 256), Image.LANCZOS).save(
            out / "icon.ico",
            "ICO",
            sizes=[(n, n) for n in ico_sizes],
        )

        # macOS refuse de regrouper l'application sans .icns.
        try:
            master.resize((1024, 1024), Image.LANCZOS).save(out / "icon.icns", "ICNS")
        except (OSError, ValueError) as e:
            print(f"  .icns ignoré ({e}) — le paquet macOS le réclamera")

        rel = out.relative_to(ROOT)
        print(f"{rel} : {len(png_sizes)} PNG + icon.ico + icon.icns")


if __name__ == "__main__":
    main()
