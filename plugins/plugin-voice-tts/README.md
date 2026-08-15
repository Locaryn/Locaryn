# Locaryn Plugin: Voice & TTS (`plugin-voice-tts`)

Official Locaryn extension providing Text-to-Speech (TTS) synthesis and voice cloning using Kokoro-82M, Qwen3-TTS, and Piper ONNX engines.

## Features
- **Fast Local TTS**: Instant generation with Kokoro-82M.
- **Multilingual Support**: Supports English, French, Japanese, Mandarin, Spanish, etc.
- **Voice Cloning**: Generate custom voice profiles from reference audio clips.

## Installation
```bash
locaryn plugin install Locaryn/plugin-voice-tts
```

## Tools Provided
- `synthesize_speech`: Converts input text to speech `.wav` file.
- `clone_voice`: Extracts speaker embeddings from a sample audio file.
