package graphics

import (
	"math"
	"testing"
)

func TestFullScreenTriangle(t *testing.T) {
	fb := NewFramebuffer(8, 8)
	// A triangle that covers the entire NDC cube face:
	// three vertices way outside [-1,1] to guarantee full coverage.
	v0 := Vertex{Pos: Vec3{0, 3, -1}, Color: [4]float64{1, 0, 0, 1}}
	v1 := Vertex{Pos: Vec3{-3, -3, -1}, Color: [4]float64{0, 1, 0, 1}}
	v2 := Vertex{Pos: Vec3{3, -3, -1}, Color: [4]float64{0, 0, 1, 1}}

	mvp := Identity()
	fb.DrawTriangle(v0, v1, v2, mvp)

	// Every pixel should have been written (alpha > 0)
	for y := range fb.Height {
		for x := range fb.Width {
			r, _, _, a := fb.Image.At(x, y).RGBA()
			_ = r
			if a == 0 {
				t.Errorf("pixel (%d,%d) not written", x, y)
			}
		}
	}
}

func TestCornerColors(t *testing.T) {
	// Place vertices at three corners of a large triangle so corner pixels
	// are dominated by one vertex color.
	fb := NewFramebuffer(100, 100)

	// Top-center: red
	v0 := Vertex{Pos: Vec3{0, 1.5, -1}, Color: [4]float64{1, 0, 0, 1}}
	// Bottom-left: green
	v1 := Vertex{Pos: Vec3{-1.5, -1.5, -1}, Color: [4]float64{0, 1, 0, 1}}
	// Bottom-right: blue
	v2 := Vertex{Pos: Vec3{1.5, -1.5, -1}, Color: [4]float64{0, 0, 1, 1}}

	fb.DrawTriangle(v0, v1, v2, Identity())

	// Bottom-left corner should be mostly green
	r, g, b, _ := fb.Image.At(5, 95).RGBA()
	if g <= r || g <= b {
		t.Errorf("bottom-left should be green-dominant, got R=%d G=%d B=%d", r>>8, g>>8, b>>8)
	}

	// Bottom-right corner should be mostly blue
	r, g, b, _ = fb.Image.At(95, 95).RGBA()
	if b <= r || b <= g {
		t.Errorf("bottom-right should be blue-dominant, got R=%d G=%d B=%d", r>>8, g>>8, b>>8)
	}

	// Top-center should be mostly red
	r, g, b, _ = fb.Image.At(50, 5).RGBA()
	if r <= g || r <= b {
		t.Errorf("top-center should be red-dominant, got R=%d G=%d B=%d", r>>8, g>>8, b>>8)
	}
}

func TestDepthTest(t *testing.T) {
	fb := NewFramebuffer(16, 16)

	// Near triangle (z=-1): red, covers whole screen
	near := Vertex{Color: [4]float64{1, 0, 0, 1}}
	nv0 := near
	nv0.Pos = Vec3{0, 3, -1}
	nv1 := near
	nv1.Pos = Vec3{-3, -3, -1}
	nv2 := near
	nv2.Pos = Vec3{3, -3, -1}

	// Far triangle (z=-0.5, but in NDC maps to larger z): green
	far := Vertex{Color: [4]float64{0, 1, 0, 1}}
	fv0 := far
	fv0.Pos = Vec3{0, 3, -0.5}
	fv1 := far
	fv1.Pos = Vec3{-3, -3, -0.5}
	fv2 := far
	fv2.Pos = Vec3{3, -3, -0.5}

	// Draw far first, then near — near should win due to depth test
	fb.DrawTriangle(fv0, fv1, fv2, Identity())
	fb.DrawTriangle(nv0, nv1, nv2, Identity())

	r, g, _, _ := fb.Image.At(8, 8).RGBA()
	if r <= g {
		t.Errorf("near (red) triangle should be visible over far (green), got R=%d G=%d", r>>8, g>>8)
	}
}

func TestDegenerateTriangle(t *testing.T) {
	fb := NewFramebuffer(8, 8)
	// Three collinear points
	v := Vertex{Pos: Vec3{0, 0, -1}, Color: [4]float64{1, 1, 1, 1}}
	v1 := v
	v1.Pos = Vec3{1, 0, -1}
	v2 := v
	v2.Pos = Vec3{2, 0, -1}

	fb.DrawTriangle(v, v1, v2, Identity())

	// No pixels should be written
	for y := range fb.Height {
		for x := range fb.Width {
			_, _, _, a := fb.Image.At(x, y).RGBA()
			if a != 0 {
				t.Errorf("degenerate triangle wrote pixel (%d,%d)", x, y)
			}
		}
	}
}

// --- Benchmarks ---

func benchmarkClear(b *testing.B, width, height int) {
	fb := NewFramebuffer(width, height)
	b.ResetTimer()
	b.ReportAllocs()
	for range b.N {
		fb.Clear()
	}
}

func BenchmarkClear640x480(b *testing.B)   { benchmarkClear(b, 640, 480) }
func BenchmarkClear800x600(b *testing.B)   { benchmarkClear(b, 800, 600) }
func BenchmarkClear1920x1080(b *testing.B) { benchmarkClear(b, 1920, 1080) }

func benchmarkDrawTriangle(b *testing.B, width, height int) {
	fb := NewFramebuffer(width, height)

	v0 := Vertex{Pos: Vec3{X: 0, Y: 0.8, Z: 0}, Color: [4]float64{1, 0, 0, 1}}
	v1 := Vertex{Pos: Vec3{X: -0.8, Y: -0.6, Z: 0}, Color: [4]float64{0, 1, 0, 1}}
	v2 := Vertex{Pos: Vec3{X: 0.8, Y: -0.6, Z: 0}, Color: [4]float64{0, 0, 1, 1}}

	aspect := float64(width) / float64(height)
	proj := Perspective(math.Pi/4, aspect, 0.1, 100)
	view := LookAt(Vec3{X: 0, Y: 0, Z: 3}, Vec3{}, Vec3{X: 0, Y: 1, Z: 0})
	model := RotateY(0.5)
	mvp := proj.Mul(view).Mul(model)

	b.ResetTimer()
	b.ReportAllocs()
	for range b.N {
		fb.Clear()
		fb.DrawTriangle(v0, v1, v2, mvp)
	}
}

func BenchmarkDrawTriangle640x480(b *testing.B)   { benchmarkDrawTriangle(b, 640, 480) }
func BenchmarkDrawTriangle800x600(b *testing.B)   { benchmarkDrawTriangle(b, 800, 600) }
func BenchmarkDrawTriangle1920x1080(b *testing.B) { benchmarkDrawTriangle(b, 1920, 1080) }

func TestBehindCamera(t *testing.T) {
	fb := NewFramebuffer(8, 8)
	proj := Perspective(math.Pi/2, 1, 0.1, 100)
	// Vertices behind camera (positive Z in view space)
	v0 := Vertex{Pos: Vec3{0, 0, 5}, Color: [4]float64{1, 1, 1, 1}}
	v1 := Vertex{Pos: Vec3{-1, -1, 5}, Color: [4]float64{1, 1, 1, 1}}
	v2 := Vertex{Pos: Vec3{1, -1, 5}, Color: [4]float64{1, 1, 1, 1}}

	fb.DrawTriangle(v0, v1, v2, proj)

	for y := range fb.Height {
		for x := range fb.Width {
			_, _, _, a := fb.Image.At(x, y).RGBA()
			if a != 0 {
				t.Errorf("behind-camera triangle wrote pixel (%d,%d)", x, y)
			}
		}
	}
}
