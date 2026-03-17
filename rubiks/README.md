# Sixel 3D Renderer

Real-time 3D software renderer outputting sixel graphics to the terminal. Currently renders a rotating triangle; intended to become a Rubik's cube visualization.

Requires a sixel-capable terminal (foot, WezTerm, xterm, mlterm) and Kitty keyboard protocol support.

## Usage

```sh
go run .   # press Escape to exit
```

## Testing

```sh
go test ./...   # run all tests
```

## Benchmarks

The rendering pipeline (rasterize → quantize → encode sixel) is benchmarked at each stage and as a full pipeline. Full-pipeline benchmarks report `fps` and `ms/frame` metrics directly.

```sh
go test -bench . ./...                        # run all benchmarks
go test -bench BenchmarkFull .                # full pipeline (reports fps + ms/frame)
go test -bench BenchmarkQuantize .            # quantization only (median-cut + nearest color)
go test -bench BenchmarkDrawTriangle ./graphics/  # rasterization only
go test -bench BenchmarkClear ./graphics/     # framebuffer clear only
go test -bench BenchmarkMVP ./graphics/       # MVP matrix construction
go test -bench BenchmarkEncode .              # sixel encoding only (no quantization)
```

### Available benchmarks

| Benchmark | Package | What it measures |
|-----------|---------|-----------------|
| `BenchmarkFullPipeline{640x480,800x600,1920x1080}` | root | MVP + Clear + DrawTriangle + EncodeImage |
| `BenchmarkQuantize{800x600,1920x1080}` | root | Median-cut quantization + nearest color mapping |
| `BenchmarkEncodeImage800x600` | root | Quantization + sixel encoding combined |
| `BenchmarkEncode{800x600,1920x1080}` | root | Sixel encoding only (pre-indexed pixels) |
| `BenchmarkDrawTriangle{640x480,800x600,1920x1080}` | graphics | Clear + rasterize one triangle |
| `BenchmarkClear{640x480,800x600,1920x1080}` | graphics | Zeroing framebuffer (RGBA + depth) |
| `BenchmarkMVPBuild` | graphics | Perspective × LookAt × RotateY |
