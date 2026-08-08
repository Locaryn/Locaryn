import sys, os, json, glob

model_path = r"{pth}"
ref_path = r"{ref}"
out_path = r"{out}"
speed = {speed}
voice_name = "{voice_name}"
voices_dir = r"{voices_dir}"

text = sys.stdin.read()

# Kokoro-82M does not support arbitrary zero-shot voice cloning from
# external audio — the .pt voice files ARE pre-computed voice style vectors.
# When a voice_reference is provided, we analyze its fundamental frequency
# (F0) and pick the built-in voice whose gender/register matches best.
# This is a best-effort heuristic; true zero-shot cloning requires XTTS.
selected_voice = voice_name
try:
    import numpy as np
    import soundfile as sf
    ref_audio, ref_sr = sf.read(ref_path)
    if ref_audio.ndim > 1:
        ref_audio = ref_audio[:, 0]
    # Estimate F0 via zero-crossing rate as a rough pitch proxy.
    # High zero-crossing rate → higher pitch → likely female voice.
    zcr = float(np.mean(np.abs(np.diff(np.sign(ref_audio)))) )
    # Kokoro naming: af_* = American female, am_* = American male
    # bf_* = British female, bm_* = British male, etc.
    is_likely_female = zcr > 0.1
    # List available voices and pick by gender match.
    voice_files = glob.glob(os.path.join(voices_dir, "*.pt"))
    candidates = []
    for vf in voice_files:
        bn = os.path.basename(vf).replace(".pt", "")
        prefix = bn[:2] if len(bn) >= 2 else ""
        vf_is_female = prefix[1] == 'f' if len(prefix) >= 2 else True
        if vf_is_female == is_likely_female:
            candidates.append(bn)
    if candidates:
        # Prefer the user's selected voice if it matches the gender;
        # otherwise pick the first matching candidate.
        if voice_name in candidates:
            selected_voice = voice_name
        else:
            selected_voice = candidates[0]
        print(f"reference F0 proxy zcr={{zcr:.4f}}, selected voice: {{selected_voice}}", file=sys.stderr)
    else:
        print(f"no gender-matched voice found, using {{voice_name}}", file=sys.stderr)
except Exception as e:
    print(f"voice analysis failed: {{e}}, using {{voice_name}}", file=sys.stderr)

voice_pt_final = os.path.join(voices_dir, selected_voice + ".pt")

try:
    from kokoro import KPipeline
    import soundfile as sf
    pipeline = KPipeline(lang_code='a')
    for i, (gs, ps, audio) in enumerate(pipeline(text, voice=selected_voice, speed=speed)):
        sf.write(out_path, audio, 24000)
        break
    print("OK")
except ImportError:
    try:
        import kokoro_onnx
        from kokoro_onnx import KokoroOnnx
        import soundfile as sf
        k = KokoroOnnx(model_path=model_path, voice_path=voice_pt_final)
        audio = k.create(text, voice=selected_voice, speed=speed, lang="en-us")
        sf.write(out_path, audio, 24000)
        print("OK")
    except ImportError:
        print("kokoro / kokoro-onnx not installed. pip install kokoro", file=sys.stderr)
        sys.exit(1)