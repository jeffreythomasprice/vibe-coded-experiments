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

## Configuration

Copy `.env.template` to `.env` and fill in your `ANTHROPIC_API_KEY`. The
template already sets `WHISPER_MODEL` (with `~` paths supported).

```
cp .env.template .env
# edit .env, set ANTHROPIC_API_KEY
```

`WHISPER_BINARY` is optional — it defaults to whatever `which whisper-cli`
resolves to. Set it only if your whisper.cpp binary has a different name or
isn't on your PATH.

The `.env` file is located by walking up the directory tree from both the
current working directory and the directory containing the executable, so a
compiled binary picks up the project's `.env` no matter where you run it from.

## Run

```
bun src/index.ts https://www.youtube.com/watch?v=...
```

## Build a standalone binary

```
bun run build              # produces ./yt-summarize
./yt-summarize https://www.youtube.com/watch?v=...   # run from anywhere
```
