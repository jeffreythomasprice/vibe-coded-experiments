# RoboWar

A robot arena combat simulator in Rust. Robots are programmed in a custom assembly language (BotASM), executed by a VM, and fight in a 2D physics arena.

## Building

```sh
cargo build
```

## Example Robots

Four example robots are included in `examples/robots/`:

| Robot | Strategy | HP | Speed | Armor | Gun Power |
|-------|----------|----|-------|-------|-----------|
| **Dumb** | Does nothing (yields every tick) | 5 | 5 | 5 | 5 |
| **Spinner** | Stands still, spins turret, fires at robots | 5 | 3 | 5 | 7 |
| **Patrol** | Moves forward, spins turret, fires and avoids walls | 5 | 5 | 5 | 5 |
| **Wanderer** | Random movement with scanning turret | 5 | 6 | 4 | 5 |

Each robot has a TOML config (name, program path, loadout) and a `.asm` program. Loadout points are allocated from a budget of 20.

## Arenas

Arena configs live in `examples/arenas/`:

- **square.toml** -- 600x600 open arena with 4 corner spawn points, no obstacles.
- **wide.toml** -- 1200x400 arena with two rectangular pillars and 4 spawn points.

An arena config is required — pass `--arena` on the command line or set `arena` in your config file.

## CLI Usage

The CLI binary is `robowar-cli`.

### Run a match

```sh
cargo run -p robowar-cli -- run --arena examples/arenas/square.toml examples/robots/spinner.toml examples/robots/patrol.toml
```

Three or more robots trigger a free-for-all:

```sh
cargo run -p robowar-cli -- run --arena examples/arenas/square.toml examples/robots/spinner.toml examples/robots/patrol.toml examples/robots/wanderer.toml examples/robots/dumb.toml
```

### Specify an arena

```sh
cargo run -p robowar-cli -- run --arena examples/arenas/wide.toml examples/robots/spinner.toml examples/robots/wanderer.toml
```

### Set tick limit and RNG seed

```sh
cargo run -p robowar-cli -- run --arena examples/arenas/square.toml --ticks 5000 --seed 99 examples/robots/spinner.toml examples/robots/patrol.toml
```

### Assemble a BotASM program

Check an `.asm` file for errors and report instruction count:

```sh
cargo run -p robowar-cli -- assemble examples/robots/spinner.asm
```

### Show robot info

Display a robot's config, loadout, and derived stats:

```sh
cargo run -p robowar-cli -- info examples/robots/spinner.toml
```

### Project config file

Instead of passing all options on the command line, create a `robowar.toml` in your working directory:

```toml
arena = "arenas/square.toml"
robots = ["robots/spinner.toml", "robots/patrol.toml"]
max_ticks = 5000
seed = 123
```

Then run with no arguments:

```sh
cd examples
cargo run -p robowar-cli -- run
```

CLI arguments override values from the config file. Use `--config` to specify a different config path (defaults to `robowar.toml` in the current directory).

#### Robot Groups

Define named groups of robots in `robowar.toml`. When a group is referenced, one robot is chosen at random (deterministically if a seed is set):

```toml
[groups]
movers = ["robots/patrol.toml", "robots/wanderer.toml"]
campers = ["robots/spinner.toml"]

robots = ["@movers", "@campers"]
```

Group references use the `@` prefix. You can also use them as CLI arguments:

```sh
cargo run -p robowar-cli -- run --arena examples/arenas/square.toml @movers @campers
```

Group paths are resolved relative to the config file directory.

## Visualizer Usage

The visualizer is a Bevy-based graphical match viewer.

### Launch a match

```sh
cargo run -p robowar-visualizer -- --arena examples/arenas/square.toml examples/robots/spinner.toml examples/robots/patrol.toml
```

### With arena and speed options

```sh
cargo run -p robowar-visualizer -- --arena examples/arenas/wide.toml --speed 2.0 examples/robots/spinner.toml examples/robots/patrol.toml
```

The `--speed` flag sets a simulation speed multiplier (default: 1.0).

### Mix of default config and overrides

```sh
cargo run -p robowar-visualizer -- --config examples/robowar-square.toml --arena examples/arenas/wide.toml
```