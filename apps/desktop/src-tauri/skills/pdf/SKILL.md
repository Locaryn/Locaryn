---
name: pdf
description: Lire, extraire, créer, fusionner, découper ou remplir des fichiers PDF.
version: 1.0.0
---
# Fichiers PDF

Tu travailles sur de vrais fichiers `.pdf` avec Python : `pypdf` pour lire et
transformer, `reportlab` pour créer.

## Méthode

1. **Vérifier l'outil** : `python --version` (sous Windows, `py --version` si
   besoin). Installer ce qui manque :
   `python -m pip install --user pypdf reportlab`.
2. **Écrire un script** (`write_file`), l'exécuter (`run_command`).
3. **Vérifier** : rouvrir le PDF produit et compter ses pages.
4. **Donner le chemin complet** du fichier produit.

## Lire et extraire

```python
from pypdf import PdfReader
r = PdfReader("document.pdf")
print(len(r.pages), "pages")
for i, page in enumerate(r.pages, 1):
    print(f"--- page {i} ---")
    print(page.extract_text())
```

Un PDF scanné (texte vide) est une image : il faut un OCR
(`pytesseract` + Tesseract installé). Le dire à la personne plutôt que
d'inventer le contenu.

## Fusionner, découper, tourner

```python
from pypdf import PdfReader, PdfWriter
w = PdfWriter()
for nom in ["a.pdf", "b.pdf"]:
    for page in PdfReader(nom).pages:
        w.add_page(page)
w.write("fusion.pdf")

# Pages 2 à 4 seulement
r = PdfReader("document.pdf"); w = PdfWriter()
for page in r.pages[1:4]:
    w.add_page(page)
w.write("extrait.pdf")

# Rotation : page.rotate(90)
```

Formulaire : `r.get_fields()` liste les champs ; remplir avec
`w.update_page_form_field_values(w.pages[0], {"nom": "Dupont"})`.

## Créer

```python
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle
from reportlab.lib import colors

doc = SimpleDocTemplate("sortie.pdf", pagesize=A4)
st = getSampleStyleSheet()
elements = [
    Paragraph("Titre", st["Title"]),
    Paragraph("Un paragraphe de texte.", st["BodyText"]),
    Spacer(1, 12),
]
t = Table([["A", "B"], ["1", "2"]])
t.setStyle(TableStyle([("GRID", (0, 0), (-1, -1), 0.5, colors.grey),
                       ("BACKGROUND", (0, 0), (-1, 0), colors.lightgrey)]))
elements.append(t)
doc.build(elements)
```

## Règles

- Ne jamais écraser le PDF d'origine : écrire un nouveau fichier.
- Pour un document mis en page richement, il est souvent plus simple de
  produire un `.docx` puis de le convertir (`libreoffice --headless
  --convert-to pdf`).
