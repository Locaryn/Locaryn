---
name: docx
description: Créer, lire ou modifier un document Word (.docx) — rapport, lettre, CV, contrat, compte rendu.
version: 1.0.0
---
# Documents Word (.docx)

Tu produis de vrais fichiers `.docx`, pas du texte à copier-coller. Passe par
Python et la bibliothèque `python-docx`.

## Méthode

1. **Vérifier l'outil** : `python --version` (sous Windows, `py --version` si
   `python` est introuvable). Puis `python -c "import docx"` ; s'il échoue,
   installe : `python -m pip install --user python-docx`.
2. **Écrire un script** (`write_file`) dans le dossier de travail, par exemple
   `generer_document.py`, puis l'exécuter (`run_command`). Un script se relit
   et se corrige ; une commande d'une ligne, non.
3. **Vérifier** le résultat en relisant le fichier :
   `python -c "import docx; d=docx.Document('sortie.docx'); print(len(d.paragraphs))"`.
4. **Donner le chemin complet** du fichier produit à la personne.

## Créer

```python
from docx import Document
from docx.shared import Pt, Cm
from docx.enum.text import WD_ALIGN_PARAGRAPH

doc = Document()
for s in doc.sections:
    s.left_margin = s.right_margin = Cm(2.5)
doc.styles["Normal"].font.name = "Calibri"
doc.styles["Normal"].font.size = Pt(11)

doc.add_heading("Titre du document", level=0)
p = doc.add_paragraph("Un paragraphe avec ")
p.add_run("du gras").bold = True
doc.add_heading("Section", level=1)
doc.add_paragraph("Premier point", style="List Bullet")
doc.add_paragraph("Étape 1", style="List Number")

t = doc.add_table(rows=1, cols=3, style="Light Grid Accent 1")
for cell, titre in zip(t.rows[0].cells, ["Colonne A", "Colonne B", "Total"]):
    cell.text = titre
ligne = t.add_row().cells
ligne[0].text, ligne[1].text, ligne[2].text = "x", "y", "z"

doc.add_page_break()
doc.save("sortie.docx")
```

Images : `doc.add_picture("image.png", width=Cm(14))`.

## Lire ou modifier un document existant

- Lire : parcourir `doc.paragraphs` (texte, `style.name`) et `doc.tables`.
- Modifier : ouvrir avec `Document("existant.docx")`, changer le texte des
  `runs` (pas `paragraph.text`, qui efface la mise en forme), puis enregistrer
  **sous un nouveau nom** sauf demande contraire — ne jamais écraser
  l'original sans accord.

## Règles

- Utiliser les styles de titre (`add_heading`) : ils donnent la table des
  matières et la navigation dans Word.
- Accents et caractères spéciaux : écrire le script en UTF-8.
- Conversion en PDF demandée : `docx2pdf` sous Windows/macOS (Word installé),
  sinon `libreoffice --headless --convert-to pdf sortie.docx`.
