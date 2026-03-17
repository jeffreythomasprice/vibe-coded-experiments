Install whisper-cpp
```
brew install whisper-cpp
```

Download a model from https://huggingface.co/ggerganov/whisper.cpp/tree/main
Look for one that follows pattern `ggml-*.bin`
```
mkdir -p ~/whisper-cpp/models
mv ~/downloads/ggml-base.en-q8_0.bin ~/whisper-cpp/models/
```

Set up `.env` using `.env.template`. Make sure to fill in the API key.

```
WHISPER_BINARY=$(which whisper-cli) WHISPER_MODEL=~/whisper-cpp/models/ggml-base.en-q8_0.bin bun src/index.ts https://www.youtube.com/watch?v=...
```
