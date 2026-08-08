import sys, os

repo_dir = {repo_dir_json}
out_path = {out_path_json}
description = {description_json}

text = sys.stdin.read()

try:
    import torch
    from parler_tts import ParlerTTSForConditionalGeneration
    from transformers import AutoTokenizer
    import soundfile as sf
except ImportError as e:
    print(f"parler-tts / transformers non installe: {{e}}", file=sys.stderr)
    sys.exit(1)

device = "cuda" if torch.cuda.is_available() else "cpu"

try:
    model = ParlerTTSForConditionalGeneration.from_pretrained(repo_dir).to(device)
    tokenizer = AutoTokenizer.from_pretrained(repo_dir)
except Exception as e:
    print(f"Impossible de charger Parler-TTS depuis {{repo_dir}}: {{e}}", file=sys.stderr)
    sys.exit(1)

try:
    input_ids = tokenizer(description, return_tensors="pt").input_ids.to(device)
    prompt_input_ids = tokenizer(text, return_tensors="pt").input_ids.to(device)
    generation = model.generate(input_ids=input_ids, prompt_input_ids=prompt_input_ids)
    audio = generation.cpu().numpy().squeeze()
    sampling_rate = getattr(model.config, "sampling_rate", 44100)
    sf.write(out_path, audio, sampling_rate)
    print("OK")
except Exception as e:
    print(f"Parler-TTS generation failed: {{e}}", file=sys.stderr)
    sys.exit(1)