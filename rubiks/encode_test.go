package main

import (
	"bytes"
	"image"
	"image/color"
	"math"
	"strings"
	"testing"

	"experiment/graphics"
)

func TestEncodeSimple(t *testing.T) {
	// 2x6 image, single band, two colors.
	width, height := 2, 6
	enc := NewEncoder(width, height)

	pal := Palette{Size: 2}
	pal.Colors[0] = [3]byte{255, 0, 0} // red
	pal.Colors[1] = [3]byte{0, 0, 255} // blue

	// Column 0 = color 0 (all 6 rows), column 1 = color 1 (all 6 rows).
	pixels := make([]byte, width*height)
	for y := range height {
		pixels[y*width+0] = 0
		pixels[y*width+1] = 1
	}

	var buf bytes.Buffer
	err := enc.Encode(&buf, pixels, pal)
	if err != nil {
		t.Fatal(err)
	}

	out := buf.String()

	// Must start with DCS and end with ST.
	if !strings.HasPrefix(out, DCS) {
		t.Errorf("missing DCS prefix, got %q", out[:min(20, len(out))])
	}
	if !strings.HasSuffix(out, ST) {
		t.Errorf("missing ST suffix")
	}

	// Should contain palette definitions for both colors.
	if !strings.Contains(out, "#0;2;") {
		t.Error("missing palette entry for color 0")
	}
	if !strings.Contains(out, "#1;2;") {
		t.Error("missing palette entry for color 1")
	}

	// Color 0 should have full column (all 6 bits = 63 = '~') for col 0.
	// sixel char for 63 = '?' + 63 = 0x3F + 0x3F = 0x7E = '~'
	// Trailing zeros are trimmed, so color 0 is just "~" (col 0 set, col 1 trimmed).
	// Color 1 has col 0 = 0, col 1 = '~' → "?~".
	if !strings.Contains(out, "#0~") {
		t.Errorf("expected color 0 to contain ~, got %q", out)
	}
	if !strings.Contains(out, "#1?~") {
		t.Errorf("expected color 1 to have ?~, got %q", out)
	}
}

func TestEncodeMultipleBands(t *testing.T) {
	// 1x12 image = 2 bands.
	width, height := 1, 12
	enc := NewEncoder(width, height)

	pal := Palette{Size: 1}
	pal.Colors[0] = [3]byte{0, 255, 0}

	pixels := make([]byte, width*height) // all color 0

	var buf bytes.Buffer
	err := enc.Encode(&buf, pixels, pal)
	if err != nil {
		t.Fatal(err)
	}

	out := buf.String()
	// Should have a '-' band separator between two bands.
	// Strip DCS header and ST trailer to count separators.
	body := out[len(DCS) : len(out)-len(ST)]
	if strings.Count(body, "-") != 1 {
		t.Errorf("expected 1 band separator, got %d in %q", strings.Count(body, "-"), body)
	}
}

func TestEncodePixelLengthError(t *testing.T) {
	enc := NewEncoder(10, 10)
	pal := Palette{Size: 1}
	err := enc.Encode(&bytes.Buffer{}, make([]byte, 50), pal)
	if err == nil {
		t.Fatal("expected error for wrong pixel length")
	}
	if _, ok := err.(ErrPixelLength); !ok {
		t.Fatalf("expected ErrPixelLength, got %T", err)
	}
}

func TestRLECompression(t *testing.T) {
	// 10 identical columns should produce RLE.
	data := make([]byte, 10)
	for i := range data {
		data[i] = 63 // all bits set → '~'
	}
	result := string(appendRLE(nil, data))
	if !strings.Contains(result, "!10~") {
		t.Errorf("expected RLE !10~, got %q", result)
	}
}

func TestRLEShortRun(t *testing.T) {
	// 3 identical values should NOT be RLE-compressed (threshold is 4).
	data := []byte{63, 63, 63}
	result := string(appendRLE(nil, data))
	if result != "~~~" {
		t.Errorf("expected ~~~, got %q", result)
	}
}

func TestRLETrailingZeroTrim(t *testing.T) {
	data := []byte{1, 0, 0, 0, 0}
	result := string(appendRLE(nil, data))
	// Should just be the single char for value 1 = '@' (? + 1 = 0x40)
	if result != "@" {
		t.Errorf("expected single '@', got %q", result)
	}
}

func TestEncodeRoundtrip(t *testing.T) {
	// Ensure encoding doesn't panic and produces valid-looking output
	// for various sizes.
	sizes := [][2]int{{1, 1}, {1, 6}, {10, 6}, {80, 24}, {320, 200}}
	for _, sz := range sizes {
		w, h := sz[0], sz[1]
		enc := NewEncoder(w, h)
		pal := Palette{Size: 4}
		pal.Colors[0] = [3]byte{0, 0, 0}
		pal.Colors[1] = [3]byte{255, 0, 0}
		pal.Colors[2] = [3]byte{0, 255, 0}
		pal.Colors[3] = [3]byte{0, 0, 255}

		pixels := make([]byte, w*h)
		for i := range pixels {
			pixels[i] = byte(i % 4)
		}

		var buf bytes.Buffer
		if err := enc.Encode(&buf, pixels, pal); err != nil {
			t.Fatalf("Encode(%dx%d): %v", w, h, err)
		}
		out := buf.String()
		if !strings.HasPrefix(out, DCS) || !strings.HasSuffix(out, ST) {
			t.Errorf("Encode(%dx%d): missing DCS/ST framing", w, h)
		}
	}
}

func TestEncodeImageGradient(t *testing.T) {
	// Create a small gradient image and ensure EncodeImage doesn't error.
	img := image.NewRGBA(image.Rect(0, 0, 32, 32))
	for y := range 32 {
		for x := range 32 {
			img.Set(x, y, color.RGBA{R: uint8(x * 8), G: uint8(y * 8), B: 128, A: 255})
		}
	}

	enc := NewEncoder(32, 32)
	var buf bytes.Buffer
	err := enc.EncodeImage(&buf, img, &QuantizeOptions{MaxColors: 16})
	if err != nil {
		t.Fatal(err)
	}
	out := buf.String()
	if !strings.HasPrefix(out, DCS) || !strings.HasSuffix(out, ST) {
		t.Error("missing DCS/ST framing")
	}
}

func TestMedianCutDistinctColors(t *testing.T) {
	// 4 distinct colors quantized to 4 should preserve all of them.
	pixels := [][3]byte{
		{255, 0, 0},
		{0, 255, 0},
		{0, 0, 255},
		{255, 255, 0},
	}
	// Repeat each to give median-cut something to work with.
	expanded := make([][3]byte, 0, 400)
	for _, p := range pixels {
		for range 100 {
			expanded = append(expanded, p)
		}
	}
	pal := medianCut(expanded, 4)
	if pal.Size != 4 {
		t.Errorf("expected 4 colors, got %d", pal.Size)
	}
}

// --- Benchmarks ---

func benchmarkEncode(b *testing.B, width, height, colors int) {
	enc := NewEncoder(width, height)
	pal := Palette{Size: colors}
	for i := range colors {
		pal.Colors[i] = [3]byte{byte(i * 5), byte(i * 7), byte(i * 11)}
	}
	pixels := make([]byte, width*height)
	for i := range pixels {
		pixels[i] = byte(i % colors)
	}
	var buf bytes.Buffer
	buf.Grow(width * height * 2)

	b.ResetTimer()
	b.ReportAllocs()
	for range b.N {
		buf.Reset()
		if err := enc.Encode(&buf, pixels, pal); err != nil {
			b.Fatal(err)
		}
	}
	b.SetBytes(int64(width * height))
}

func BenchmarkEncode800x600(b *testing.B)   { benchmarkEncode(b, 800, 600, 64) }
func BenchmarkEncode1920x1080(b *testing.B) { benchmarkEncode(b, 1920, 1080, 64) }

func benchmarkQuantize(b *testing.B, width, height int) {
	img := image.NewRGBA(image.Rect(0, 0, width, height))
	for y := range height {
		for x := range width {
			img.SetRGBA(x, y, color.RGBA{
				R: uint8(x % 256), G: uint8(y % 256), B: uint8((x + y) % 256), A: 255,
			})
		}
	}
	b.ResetTimer()
	b.ReportAllocs()
	for range b.N {
		ImageToPaletted(img, 64)
	}
	b.SetBytes(int64(width * height))
}

func BenchmarkQuantize800x600(b *testing.B)   { benchmarkQuantize(b, 800, 600) }
func BenchmarkQuantize1920x1080(b *testing.B) { benchmarkQuantize(b, 1920, 1080) }

func BenchmarkEncodeImage800x600(b *testing.B) {
	img := image.NewRGBA(image.Rect(0, 0, 800, 600))
	for y := range 600 {
		for x := range 800 {
			img.Set(x, y, color.RGBA{
				R: uint8(x % 256), G: uint8(y % 256), B: uint8((x + y) % 256), A: 255,
			})
		}
	}
	enc := NewEncoder(800, 600)
	var buf bytes.Buffer
	buf.Grow(800 * 600 * 2)

	b.ResetTimer()
	b.ReportAllocs()
	for range b.N {
		buf.Reset()
		if err := enc.EncodeImage(&buf, img, &QuantizeOptions{MaxColors: 64}); err != nil {
			b.Fatal(err)
		}
	}
	b.SetBytes(800 * 600)
}

func benchmarkFullPipeline(b *testing.B, width, height int) {
	fb := graphics.NewFramebuffer(width, height)
	enc := NewEncoder(width, height)
	var buf bytes.Buffer
	buf.Grow(width * height * 2)

	vertices := []graphics.Vertex{
		{Pos: graphics.Vec3{X: 0, Y: 0.8, Z: 0}, Color: [4]float64{1, 0, 0, 1}},
		{Pos: graphics.Vec3{X: -0.8, Y: -0.6, Z: 0}, Color: [4]float64{0, 1, 0, 1}},
		{Pos: graphics.Vec3{X: 0.8, Y: -0.6, Z: 0}, Color: [4]float64{0, 0, 1, 1}},
	}
	opts := &QuantizeOptions{MaxColors: 64}

	aspect := float64(width) / float64(height)
	proj := graphics.Perspective(math.Pi/4, aspect, 0.1, 100)
	view := graphics.LookAt(
		graphics.Vec3{X: 0, Y: 0, Z: 3},
		graphics.Vec3{},
		graphics.Vec3{X: 0, Y: 1, Z: 0},
	)
	model := graphics.RotateY(0.5)
	mvp := proj.Mul(view).Mul(model)

	b.ResetTimer()
	b.ReportAllocs()
	for range b.N {
		buf.Reset()
		if err := enc.RenderFrame(&buf, fb, vertices, mvp, opts); err != nil {
			b.Fatal(err)
		}
	}

	elapsed := b.Elapsed()
	if elapsed > 0 && b.N > 0 {
		msPerFrame := float64(elapsed.Milliseconds()) / float64(b.N)
		fps := 1000.0 / msPerFrame
		b.ReportMetric(fps, "fps")
		b.ReportMetric(msPerFrame, "ms/frame")
	}
}

func BenchmarkFullPipeline640x480(b *testing.B)   { benchmarkFullPipeline(b, 640, 480) }
func BenchmarkFullPipeline800x600(b *testing.B)   { benchmarkFullPipeline(b, 800, 600) }
func BenchmarkFullPipeline1920x1080(b *testing.B) { benchmarkFullPipeline(b, 1920, 1080) }
