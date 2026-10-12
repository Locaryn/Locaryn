---
name: xlsx
description: Créer, lire ou modifier un classeur Excel (.xlsx/.csv) — tableaux, formules, graphiques, nettoyage de données.
version: 1.0.0
---
# Classeurs Excel (.xlsx)

Tu produis de vrais classeurs avec Python : `openpyxl` pour les formules, la
mise en forme et les graphiques ; `pandas` pour analyser ou nettoyer des
données.

## Méthode

1. **Vérifier l'outil** : `python --version` (sous Windows, `py --version` si
   besoin). Installer ce qui manque :
   `python -m pip install --user openpyxl pandas`.
2. **Écrire un script** (`write_file`), l'exécuter (`run_command`).
3. **Vérifier** : rouvrir le classeur et afficher quelques cellules.
4. **Donner le chemin complet** du fichier produit.

## Créer avec formules et mise en forme

```python
from openpyxl import Workbook
from openpyxl.styles import Font, PatternFill, Alignment
from openpyxl.chart import BarChart, Reference

wb = Workbook()
ws = wb.active
ws.title = "Ventes"
ws.append(["Mois", "Montant"])
for mois, montant in [("Janv.", 1200), ("Févr.", 1500), ("Mars", 900)]:
    ws.append([mois, montant])
ws["A5"] = "Total"
ws["B5"] = "=SUM(B2:B4)"          # une vraie formule, recalculée par Excel

for c in ws[1]:
    c.font = Font(bold=True, color="FFFFFF")
    c.fill = PatternFill("solid", fgColor="4472C4")
    c.alignment = Alignment(horizontal="center")
ws.column_dimensions["A"].width = 14
for ligne in ws.iter_rows(min_row=2, min_col=2, max_col=2):
    for c in ligne:
        c.number_format = '#,##0.00 €'
ws.freeze_panes = "A2"

graphique = BarChart()
graphique.title = "Ventes par mois"
graphique.add_data(Reference(ws, min_col=2, min_row=1, max_row=4), titles_from_data=True)
graphique.set_categories(Reference(ws, min_col=1, min_row=2, max_row=4))
ws.add_chart(graphique, "D2")

wb.save("sortie.xlsx")
```

## Lire, analyser, nettoyer

```python
import pandas as pd
df = pd.read_excel("donnees.xlsx", sheet_name=None)   # toutes les feuilles
for nom, feuille in df.items():
    print(nom, feuille.shape); print(feuille.head())
```

Nettoyage : `dropna`, `drop_duplicates`, conversion des dates
(`pd.to_datetime`), puis `df.to_excel("propre.xlsx", index=False)`.
CSV : `pd.read_csv(..., sep=None, engine="python")` devine le séparateur.

## Règles

- Préférer des **formules** à des valeurs calculées en Python quand le
  classeur doit rester vivant.
- `openpyxl` ne calcule pas les formules : `data_only=True` rend la dernière
  valeur enregistrée par Excel (vide pour un fichier jamais ouvert).
- Ne jamais écraser le fichier d'origine sans accord : écrire une copie.
