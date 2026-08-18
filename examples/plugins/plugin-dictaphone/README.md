# Plugin Dictaphone — Locaryn

Extension de dictée vocale et de transcription audio en temps réel pour le champ de saisie de Locaryn.

## Fonctionnement

Ce plugin utilise la nouvelle architecture **Pluggable UI Slots** de Locaryn :
- **Slot cible** : `composer.before_send` (se positionne automatiquement à gauche du bouton *Envoyer*).
- **Rendu** : Custom Web Component `<locaryn-dictaphone-btn>`.
- **SDK Bridge** : Interagit avec `window.locaryn.chat.insertText()` et `window.locaryn.ui.showToast()`.
- **Zéro modification de code natif** : Le plugin est 100% autonome et distribuable indépendamment.

## Manifeste (`plugin.json`)

```json
{
  "$schema": "https://locaryn.dev/schema/plugin.json/v0.1",
  "apiVersion": "0.1",
  "name": "plugin-dictaphone",
  "version": "1.0.0",
  "description": "Dictaphone et transcription vocale en temps réel pour le chat",
  "author": "Locaryn Contributor",
  "license": "Apache-2.0",
  "capabilities": ["voice-tts"],
  "ui_contributions": {
    "slots": [
      {
        "id": "dictaphone-mic-btn",
        "slot": "composer.before_send",
        "order": 10,
        "type": "custom-element",
        "entry": "dist/ui.js",
        "tag": "locaryn-dictaphone-btn",
        "hint": "Dicter votre message (cliquer pour écouter)"
      }
    ]
  }
}
```
