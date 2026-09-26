# diablo-like

## Build and run

Desktop:

    cargo run -p desktop
    cargo run -p desktop --release

Web (WebGL2, served on http://localhost:8000):

    cd web && trunk serve
    cd web && trunk build --release

Tests:

    cargo test

## config.toml

`config.toml` is read from the current directory, then from the directory containing the
executable; the first one found wins, and a missing file just means defaults. It configures
logging only:

    [logging]
    filter = "warn,engine=trace,desktop=trace"
    file = "diablo-like.log"

`RUST_LOG` overrides `filter`. With `file` set, logs go to both the file and stdout.
Unknown keys are an error. Web ignores `file` and uses the built-in defaults.
