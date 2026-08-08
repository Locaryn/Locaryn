import sys, os

model_dir = {repo_dir_json}
out_path = {out_path_json}
language = "{lang}"
ref_path = {ref_json}

text = sys.stdin.read()

try:
    from TTS.api import TTS
    import torch
except ImportError:
    print("coqui-tts (TTS) not installed. pip install coqui-tts", file=sys.stderr)
    sys.exit(1)

device = "cuda" if torch.cuda.is_available() else "cpu"

# Load XTTS from the local checkpoint directory (pass the directory, not
# the individual .pth — coqui-tts expects a model directory with config.json).
tts = TTS(model_path=model_dir, config_path=os.path.join(model_dir, "config.json")).to(device)

if ref_path and ref_path != "None":
    # Voice cloning mode: use the reference audio as speaker_wav.
    tts.tts_to_file(
        text=text,
        file_path=out_path,
        speaker_wav=ref_path,
        language=language,
    )
else:
    # No reference: use the first available speaker.
    speakers = tts.speakers if hasattr(tts, 'speakers') else ["speaker"]
    tts.tts_to_file(
        text=text,
        file_path=out_path,
        speaker=speakers[0] if speakers else None,
        language=language,
    )

print("OK")