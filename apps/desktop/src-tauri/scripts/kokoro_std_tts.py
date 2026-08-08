import sys, os

model_path = r"{pth}"
voice_pt = r"{voice_pt}"
out_path = r"{out}"
speed = {speed}
voice_name = "{voice_name}"

text = sys.stdin.read()

try:
    from kokoro import KPipeline
    import soundfile as sf
    pipeline = KPipeline(lang_code='a')
    for i, (gs, ps, audio) in enumerate(pipeline(text, voice=voice_name, speed=speed)):
        sf.write(out_path, audio, 24000)
        break  # first segment is enough for short text
    print("OK")
except ImportError:
    # Fallback: kokoro-onnx
    try:
        import kokoro_onnx
        from kokoro_onnx import KokoroOnnx
        import soundfile as sf
        k = KokoroOnnx(model_path=model_path, voice_path=voice_pt)
        audio = k.create(text, voice=voice_name, speed=speed, lang="en-us")
        sf.write(out_path, audio, 24000)
        print("OK")
    except ImportError:
        print("kokoro / kokoro-onnx not installed", file=sys.stderr)
        sys.exit(1)