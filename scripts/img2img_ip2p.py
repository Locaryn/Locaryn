#!/usr/bin/env python3
"""
img2img_ip2p.py — Edition d'image par instruction naturelle via InstructPix2Pix.

Usage:
    python img2img_ip2p.py <input_image> <output_image> "<instruction>" [steps] [cfg_text] [cfg_image]

Exemples:
    python img2img_ip2p.py test.jpg output_brown.png "change the white t-shirt to brown" 30 7.5 1.2
    python img2img_ip2p.py input.png edited.png "make it look like a watercolor painting" 20 7.5 1.5

Le modèle InstructPix2Pix (timbrooks/instruct-pix2pix-005220) prend une image
+ une instruction en langage naturel et produit l'image éditée. Pas besoin de
prompt décrivant l'image entière — juste l'instruction de modification.
"""

import sys
import os

# Fix encoding for Windows console
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")

def main():
    if len(sys.argv) < 4:
        print("Usage: img2img_ip2p.py <input> <output> <instruction> [steps] [cfg_text] [cfg_image]", file=sys.stderr)
        sys.exit(1)

    input_path = sys.argv[1]
    output_path = sys.argv[2]
    instruction = sys.argv[3]
    steps = int(sys.argv[4]) if len(sys.argv) > 4 else 30
    cfg_text = float(sys.argv[5]) if len(sys.argv) > 5 else 7.5
    cfg_image = float(sys.argv[6]) if len(sys.argv) > 6 else 1.2

    if not os.path.isfile(input_path):
        print(f"ERROR: input image not found: {input_path}", file=sys.stderr)
        sys.exit(1)

    # Import heavy libs only after arg validation
    import torch
    from diffusers import StableDiffusionInstructPix2PixPipeline
    from PIL import Image

    print(f"Loading InstructPix2Pix model...", file=sys.stderr, flush=True)

    model_id = "timbrooks/instruct-pix2pix"
    pipe = StableDiffusionInstructPix2PixPipeline.from_pretrained(
        model_id,
        torch_dtype=torch.float16 if torch.cuda.is_available() else torch.float32,
        safety_checker=None,
        requires_safety_checker=False,
    )

    if torch.cuda.is_available():
        # NOTE: enable_model_cpu_offload() manages device placement itself.
        # Calling .to("cuda") before it causes a conflict — diffusers docs
        # say: "If you use enable_model_cpu_offload(), do not use .to('cuda')."
        try:
            pipe.enable_model_cpu_offload()
            print("CPU offload enabled (manages CUDA placement internally)", file=sys.stderr, flush=True)
        except Exception:
            # Fallback: move entire pipeline to CUDA at once
            pipe = pipe.to("cuda")
            print("Moved pipeline to CUDA directly", file=sys.stderr, flush=True)
        # Enable memory-efficient attention if available
        try:
            pipe.enable_xformers_memory_efficient_attention()
            print("xformers enabled", file=sys.stderr, flush=True)
        except Exception:
            pass
    else:
        pipe = pipe.to("cpu")

    print(f"Loading input image: {input_path}", file=sys.stderr, flush=True)
    input_image = Image.open(input_path).convert("RGB")

    # Resize to a dimension compatible with SD (must be divisible by 8)
    # Keep aspect ratio, cap at 512 for speed
    w, h = input_image.size
    max_dim = 512
    if max(w, h) > max_dim:
        ratio = max_dim / max(w, h)
        new_w = int(w * ratio) // 8 * 8
        new_h = int(h * ratio) // 8 * 8
        input_image = input_image.resize((new_w, new_h), Image.LANCZOS)

    print(f"Image size: {input_image.size}", file=sys.stderr, flush=True)
    print(f"Instruction: {instruction}", file=sys.stderr, flush=True)
    print(f"Steps={steps}, cfg_text={cfg_text}, cfg_image={cfg_image}", file=sys.stderr, flush=True)
    print(f"Generating...", file=sys.stderr, flush=True)

    # Set seed for reproducibility
    generator = torch.Generator(device="cuda" if torch.cuda.is_available() else "cpu").manual_seed(42)

    output = pipe(
        prompt=instruction,
        image=input_image,
        num_inference_steps=steps,
        image_guidance_scale=cfg_image,
        guidance_scale=cfg_text,
        generator=generator,
    )

    result = output.images[0]

    # Ensure output dir exists
    out_dir = os.path.dirname(output_path)
    if out_dir and not os.path.isdir(out_dir):
        os.makedirs(out_dir, exist_ok=True)

    result.save(output_path, "PNG")
    print(f"Saved: {output_path}", file=sys.stderr, flush=True)

    # Print the absolute path for the caller to pick up
    print(os.path.abspath(output_path), flush=True)

if __name__ == "__main__":
    main()
