# Qwen3-TTS inference
import sys, os, json, subprocess, tempfile

repo_dir = {repo_dir_json}
out_path = {out_path_json}
lang = {lang_json}
speed = {speed}
voice_ref = {ref_json}
is_custom = {is_custom_py}
desc_prompt = {desc_prompt_json}

text = sys.stdin.read()

# VoiceDesign variant: prepend voice description to text
repo_dirname = os.path.basename(repo_dir.rstrip('/\\')).lower()
is_voicedesign = 'voicedesign' in repo_dirname
if desc_prompt and is_voicedesign:
    text = f"[Voice style: {{desc_prompt}}] {{text}}"

def report(pct, msg):
    print(json.dumps({{'progress': pct, 'detail': msg}}), flush=True)

report(10, "Qwen3-TTS : chargement du modele")

try:
    import torch
    from transformers import (
        AutoProcessor,
        Qwen2AudioForConditionalGeneration,
    )
except ImportError:
    print(
        json.dumps({{'progress': -1, 'detail': "Installation transformers..."}}),
        flush=True,
    )
    subprocess.check_call(
        [sys.executable, "-m", "pip", "install", "-q", "torch", "transformers", "soundfile"]
    )
    from transformers import (
        AutoProcessor,
        Qwen2AudioForConditionalGeneration,
    )

device = "cuda" if torch.cuda.is_available() else "cpu"

# For Qwen2AudioForConditionalGeneration (or newer arch)
try:
    processor = AutoProcessor.from_pretrained(repo_dir, trust_remote_code=True)
    model = Qwen2AudioForConditionalGeneration.from_pretrained(
        repo_dir, torch_dtype=torch.float16 if device == "cuda" else torch.float32,
        trust_remote_code=True,
    ).to(device)
except Exception:
    # Fallback: load via AutoModel for unknown model types
    from transformers import AutoModelForCausalLM
    processor = AutoProcessor.from_pretrained(repo_dir, trust_remote_code=True)
    model = AutoModelForCausalLM.from_pretrained(
        repo_dir, torch_dtype=torch.float16 if device == "cuda" else torch.float32,
        trust_remote_code=True,
    ).to(device)

report(30, "Qwen3-TTS : modele charge")

if voice_ref and is_custom and os.path.isfile(voice_ref):
    # Voice cloning mode — load reference audio
    import soundfile as sf
    import numpy as np
    audio_array, sr = sf.read(voice_ref)
    if sr != 16000:
        # Resample to 16kHz via scipy (avoids heavy librosa dep)
        # Scipy peut ne pas etre installe — on utilise torch interpolate
        import torch.nn.functional as F
        audio_t = torch.from_numpy(audio_array).float().view(1, 1, -1)
        new_len = int(len(audio_array) * 16000.0 / sr)
        audio_t = F.interpolate(audio_t, size=new_len, mode='linear', align_corners=False)
        audio_array = audio_t.view(-1).numpy()
    kwargs = {{}}
    if lang:
        kwargs['language'] = lang
    inputs = processor(
        text=[text],
        audio=[audio_array],
        sampling_rate=16000,
        return_tensors="pt",
        **kwargs,
    )
else:
    # Standard TTS or Voice Design (no reference needed)
    kwargs = {{}}
    if lang:
        kwargs['language'] = lang
    inputs = processor(text=[text], return_tensors="pt", **kwargs)

inputs = {{k: v.to(device) if hasattr(v, 'to') else v for k, v in inputs.items()}}

report(50, "Qwen3-TTS : generation audio")

# Generate audio tokens
# max_new_tokens: ~25 tokens per second of audio at 12Hz
max_len = max(256, int(len(text.split()) * 8))
with torch.no_grad():
    generated_ids = model.generate(
        **inputs,
        max_new_tokens=max_len,
        do_sample=True,
        temperature=0.7 + (1.0 - speed) * 0.3,  # lower speed = more conservative
    )

report(80, "Qwen3-TTS : decodage audio")

# Decode audio from generated tokens
audio_channels = getattr(
    getattr(model.config, 'audio_config', None), 'audio_channels', 1
)
audio_values = processor.batch_decode(
    generated_ids, audio_length=audio_channels
)
if isinstance(audio_values, list):
    audio_values = audio_values[0]

import soundfile as sf
if hasattr(audio_values, 'numpy'):
    audio_np = audio_values.numpy()
elif hasattr(audio_values, 'detach'):
    audio_np = audio_values.detach().cpu().numpy()
else:
    audio_np = audio_values

sf.write(out_path, audio_np, samplerate=24000)

report(100, "Qwen3-TTS : termine")