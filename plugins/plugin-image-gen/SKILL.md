---
name: image-generation
description: Generate or edit images with the plugin-owned stable-diffusion.cpp runtime when the user asks for visuals, illustrations, icons, logos or image transformations.
---

# Image Generation Skill

Use the plugin MCP tools rather than application commands:

- `list_image_models` lists only installed diffusion checkpoints.
- `install_image_model` downloads a checkpoint and its required companions from HuggingFace.
- `generate_image` renders locally through the extension's stable-diffusion.cpp process.

For `generate_image`, provide:

- `prompt`: detailed visual description;
- `model`: a model returned by `list_image_models`;
- `width` / `height`: a value between 64 and 2048;
- `steps` and `cfg_scale` when the user asks for custom sampling;
- `input_image` as a `data:image/...;base64,...` URL for image-to-image editing;
- `variants` between 1 and 8 when several alternatives are useful.

The extension owns the model catalog, VAE/text-encoder companions, process invocation,
output files and Studio/composer interface. Locaryn itself does not expose an image
engine or an image-generation command.
