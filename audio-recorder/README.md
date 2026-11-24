# audio-recorder

Records every enabled input device and everything played to every enabled output device (PipeWire, Linux), into per-device segments with a session timeline. Segments are written as lossless FLAC while recording (`compression.live`; set it to false for WAV) and optionally transcoded to Opus once the recording stops.

## Requirements

PipeWire, a Rust toolchain, `clang`, libpipewire and WebKitGTK 4.1 headers, libayatana-appindicator. Transcription builds whisper.cpp with CUDA by default (CUDA toolkit and `cmake`; pass `--no-default-features` to the transcribe crate for CPU only). For the GUI: `trunk`, `cargo-tauri` and the `wasm32-unknown-unknown` target.

## Build and run

```
cargo test                          # schema, core, cli
cargo run -p audio-recorder-cli -- devices --watch
cargo run -p audio-recorder-cli -- record      # Ctrl-C to stop
./dev.sh                                       # GUI in dev mode (live reload)
cd crates/app && cargo tauri build --no-bundle # release binary in target/release
```

The GUI sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` itself (blank window on NVIDIA/X11 otherwise); if the window is still blank try `WEBKIT_DISABLE_COMPOSITING_MODE=1`.

The binaries find the sherpa-onnx libraries next to themselves (`$ORIGIN`); copy `libsherpa-onnx-c-api.so` and `libonnxruntime.so` from `target/<profile>` when moving a binary.

CLI subcommands: `config`, `devices`, `enable`, `disable`, `record`, `sessions`, `play`, `mixdown`, `transcribe`, `transcript`, `compress`, `tasks`, `models`.

While recording, speech is transcribed live with a smaller model (`record --no-live` or `transcription.live.enabled = false` to turn off) and saved as `transcript.live.json`. When the recording stops, the whole session is transcribed again with the main model (`transcription.auto`) and that transcript replaces the live draft. Models download on first use, or run `models download`.

```
audio-recorder transcribe latest [--replace]      # queue and run
audio-recorder transcript latest --source --speaker --timestamps [--json] [-o file]
audio-recorder summarize latest [--replace]       # LLM summary of the transcript (also runs after each transcription)
audio-recorder summary latest [-o file]
audio-recorder compress latest                    # WAV tracks to FLAC/Opus, FLAC to Opus (also runs automatically)
audio-recorder tasks [--watch] | tasks cancel <id> | tasks clear | tasks run
```

## Configuration

First existing of `./config.toml`, `<binary dir>/config.toml`, `~/.config/audio-recorder/config.toml`.

`config.example.toml` lists every option with its default. It is embedded in the binaries as the built-in defaults, so a `config.toml` only needs the keys it changes.
