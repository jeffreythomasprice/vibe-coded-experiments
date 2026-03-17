# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Real-time 3D software renderer outputting sixel graphics to the terminal. Currently renders a rotating triangle; intended to become a Rubik's cube visualization. Requires a sixel-capable terminal (foot, WezTerm, xterm, mlterm) and Kitty keyboard protocol support.

## Commands

```sh
go vet ./...                            # check for compilation errors (no binary output)
go run .                                # fullscreen sixel demo (press Escape to exit)
go test ./...                           # run all tests
go test -run TestEncodeSimple           # run a single test
go test -bench . ./...                  # run all benchmarks (both packages)
go test -bench BenchmarkFull .          # full-pipeline benchmarks only (reports fps + ms/frame)
go test -bench BenchmarkDrawTriangle ./graphics/  # graphics rasterization benchmarks
go test -bench BenchmarkQuantize .      # quantization-only benchmarks
```

## Architecture

Two layers: a sixel encoder (root package) and a 3D graphics pipeline (`graphics/` package). Uses `golang.org/x/term` for raw mode.

### Sixel encoder (root package)

**Pipeline:** `image.Image` → median-cut quantization (`quantize.go`) → indexed pixel buffer → scatter algorithm → RLE-encoded sixel output (`encode.go`, `rle.go`).

- `sixel.go` — Core types: `Encoder` (reusable, pre-allocated buffers), `Palette`, constants (DCS/ST escape sequences, band height)
- `encode.go` — `Encoder.Encode()`: takes indexed pixels + palette, produces sixel output using column-first scatter algorithm (one pass per band, then RLE per active color)
- `quantize.go` — `Encoder.EncodeImage()`: convenience wrapper that quantizes an `image.Image` via median-cut then calls `Encode`. Also has `ImageToPaletted()` for separate quantize-then-encode workflows
- `rle.go` — RLE compression for sixel band rows; `appendDecimal` for allocation-free integer formatting
- `terminal.go` — `GetTermSize()` via TIOCGWINSZ ioctl, `TermSize.PixelSize()` with cell-estimate fallback
- `keyboard.go` — Kitty keyboard protocol parser: `ParseKeyEvent()` for CSI u sequences

**Encoder is reusable but not concurrent** — create one per goroutine. It pre-allocates scatter buffers (`[256][width]byte`) and reuses the output buffer across frames.

### 3D graphics pipeline (`graphics/` package)

Software rasterizer with a standard MVP (Model-View-Projection) transformation pipeline.

- `vec3.go` — `Vec3` type with Add, Sub, Scale, Dot, Cross, Normalize
- `mat4.go` — `Mat4` (column-major 4×4) with Translate, RotateY, Perspective, LookAt, and point transform with perspective division
- `rasterizer.go` — `Framebuffer` (RGBA image + depth buffer), `Vertex` (position + color), `DrawTriangle()` using barycentric interpolation with depth testing

### Demo (`main.go`)

Fullscreen render loop at 30 FPS: builds MVP matrix (perspective + camera + Y-rotation), rasterizes geometry into a `Framebuffer`, quantizes to sixel, outputs to terminal. Escape to exit.
