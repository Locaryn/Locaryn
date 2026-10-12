---
name: pptx
description: Créer, lire ou modifier une présentation PowerPoint (.pptx) — diapositives, titres, puces, images, tableaux.
version: 1.0.0
---
# Présentations PowerPoint (.pptx)

Tu produis de vraies présentations avec Python et `python-pptx`.

## Méthode

1. **Vérifier l'outil** : `python --version` (sous Windows, `py --version` si
   besoin). Installer : `python -m pip install --user python-pptx`.
2. **Préparer le plan** : une idée par diapositive, 3 à 6 puces courtes au
   plus. Le montrer brièvement avant de générer si la demande est floue.
3. **Écrire un script** (`write_file`), l'exécuter (`run_command`).
4. **Vérifier** : rouvrir le fichier et lister les titres des diapositives.
5. **Donner le chemin complet** du fichier produit.

## Créer

```python
from pptx import Presentation
from pptx.util import Inches, Pt
from pptx.dml.color import RGBColor

prs = Presentation()
prs.slide_width, prs.slide_height = Inches(13.333), Inches(7.5)   # 16:9

# Diapositive de titre
s = prs.slides.add_slide(prs.slide_layouts[0])
s.shapes.title.text = "Titre de la présentation"
s.placeholders[1].text = "Sous-titre — date"

# Diapositive à puces
s = prs.slides.add_slide(prs.slide_layouts[1])
s.shapes.title.text = "Points clés"
corps = s.placeholders[1].text_frame
corps.text = "Premier point"
for texte in ["Deuxième point", "Troisième point"]:
    p = corps.add_paragraph(); p.text = texte; p.level = 0
for p in corps.paragraphs:
    for r in p.runs:
        r.font.size = Pt(24)

# Image
s = prs.slides.add_slide(prs.slide_layouts[5])
s.shapes.title.text = "Illustration"
# s.shapes.add_picture("image.png", Inches(1), Inches(1.5), height=Inches(5))

# Tableau
s = prs.slides.add_slide(prs.slide_layouts[5])
s.shapes.title.text = "Chiffres"
t = s.shapes.add_table(3, 2, Inches(1), Inches(1.8), Inches(8), Inches(2)).table
for i, (a, b) in enumerate([("Indicateur", "Valeur"), ("CA", "1,2 M€"), ("Clients", "340")]):
    t.cell(i, 0).text, t.cell(i, 1).text = a, b

# Notes de l'orateur
s.notes_slide.notes_text_frame.text = "Ce que je dis sur cette diapositive."

prs.save("sortie.pptx")
```

## Lire ou modifier

```python
from pptx import Presentation
prs = Presentation("existante.pptx")
for i, s in enumerate(prs.slides, 1):
    textes = [sh.text_frame.text for sh in s.shapes if sh.has_text_frame]
    print(i, textes)
```

Pour garder l'identité visuelle d'un modèle de la personne, partir de son
fichier : `Presentation("modele.pptx")` et utiliser ses `slide_layouts`.

## Règles

- Peu de texte par diapositive ; le détail va dans les notes.
- Ne jamais écraser le fichier d'origine sans accord : écrire une copie.
- Export PDF : `libreoffice --headless --convert-to pdf sortie.pptx`.
