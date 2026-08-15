# Locaryn Plugin: Image Generation (`plugin-image-gen`)

Official Locaryn extension providing text-to-image synthesis using local diffusion engines (Stable Diffusion, Flux, SDXL, Z-Image).

## Features
- **Diffusion Support**: Works with `.gguf` and `.safetensors` checkpoints.
- **Auto-Sampling**: Automatically tunes sampling steps and CFG scale depending on model family (Flow-matching vs standard diffusion).
- **Studio Integration**: Adds the "Image" tab to Locaryn's Studio view when enabled.

## Installation
```bash
locaryn plugin install Locaryn/plugin-image-gen
```

## Tools Provided
- `generate_image`: Generates an image file from a text prompt and optional negative prompt.
