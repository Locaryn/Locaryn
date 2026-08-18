# Locaryn Plugin: Model Training, LoRA & Obliteration Studio (`plugin-model-training`)

Official Locaryn extension for model fine-tuning, LoRA adapters management, quantization precision, and **Representation Engineering (RepE) Model Obliteration & Uncensoring**.

## ✨ Features
- **LoRA & QLoRA Fine-Tuning**: Train, hot-load and merge `.gguf` LoRA adapters directly on your local datasets.
- **Model Obliteration & Uncensoring (RepE)**: Extract refusal activation vectors across target tensor layers and perform orthogonal weight ablation to remove safety refusals for AI research and uncensored local inference.
- **UI Contribution**: Injects the dedicated top-level **« Entraînement & Oblitération »** Studio into Locaryn's main navigation drawer.
- **GGUF Model Quantization & Export**: Save abliterated or fine-tuned models directly into your active model repository.

> **Disclaimer**: Refusal obliteration is provided for AI safety research and model analysis. The user assumes full responsibility for the outputs and behavior of modified models.

## 📦 Installation
```bash
locaryn plugin install Locaryn/plugin-model-training
```

