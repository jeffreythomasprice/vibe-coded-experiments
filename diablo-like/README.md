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
executable; the first one found wins, and a missing file just means defaults. Unknown keys
are an error. Web ignores the file entirely and uses the built-in defaults.

    [logging]
    filter = "warn,engine=trace,desktop=trace"
    file = "diablo-like.log"

`RUST_LOG` overrides `filter`. With `file` set, logs go to both the file and stdout.

    [input.bindings]
    move_up = ["key:W", "key:Up", "pad_axis:LeftStickY+"]
    quit = ["key:Escape", "pad_button:Start"]

Bindings listed for an action replace its defaults; `[]` unbinds it. Tokens are
`key:`/`mouse_button:`/`mouse_axis:`/`pad_button:`/`pad_axis:` followed by a name (axis
tokens need a trailing `+`/`-`); a key or pad control with no name can still be bound by
its raw platform code, e.g. `key:native.xkb.38` or `pad_button:code.704` — check the log
(with debug logging on) for the code.
