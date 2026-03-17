package graphics

import (
	"math"
	"testing"
)

func TestFullScreenTriangle(t *testing.T) {
	fb := NewFramebuffer(8, 8)
	// A triangle that covers the entire NDC cube face:
	// three vertices way outside [-1,1] to guarantee full coverage.
	p0 := Vec3{0, 3, -1}
	p1 := Vec3{-3, -3, -1}
	p2 := Vec3{3, -3, -1}

	mvp := Identity()
	fb.DrawTriangle(p0, p1, p2, 1, mvp)

	// Every pixel should have been written (color index > 0)
	for y := range fb.Height {
		for x := range fb.Width {
			if fb.Pixels[y*fb.Width+x] == 0 {
				t.Errorf("pixel (%d,%d) not written", x, y)
			}
		}
	}
}

func TestDepthTest(t *testing.T) {
	fb := NewFramebuffer(16, 16)

	// Near triangle (z=-1): color 1
	np0 := Vec3{0, 3, -1}
	np1 := Vec3{-3, -3, -1}
	np2 := Vec3{3, -3, -1}

	// Far triangle (z=-0.5, but in NDC maps to larger z): color 2
	fp0 := Vec3{0, 3, -0.5}
	fp1 := Vec3{-3, -3, -0.5}
	fp2 := Vec3{3, -3, -0.5}

	// Draw far first, then near — near should win due to depth test
	fb.DrawTriangle(fp0, fp1, fp2, 2, Identity())
	fb.DrawTriangle(np0, np1, np2, 1, Identity())

	idx := fb.Pixels[8*fb.Width+8]
	if idx != 1 {
		t.Errorf("near triangle (color 1) should be visible over far (color 2), got color %d", idx)
	}
}

func TestDegenerateTriangle(t *testing.T) {
	fb := NewFramebuffer(8, 8)
	// Three collinear points
	p0 := Vec3{0, 0, -1}
	p1 := Vec3{1, 0, -1}
	p2 := Vec3{2, 0, -1}

	fb.DrawTriangle(p0, p1, p2, 1, Identity())

	// No pixels should be written
	for y := range fb.Height {
		for x := range fb.Width {
			if fb.Pixels[y*fb.Width+x] != 0 {
				t.Errorf("degenerate triangle wrote pixel (%d,%d)", x, y)
			}
		}
	}
}

func TestBehindCamera(t *testing.T) {
	fb := NewFramebuffer(8, 8)
	proj := Perspective(math.Pi/2, 1, 0.1, 100)
	// Vertices behind camera (positive Z in view space)
	p0 := Vec3{0, 0, 5}
	p1 := Vec3{-1, -1, 5}
	p2 := Vec3{1, -1, 5}

	fb.DrawTriangle(p0, p1, p2, 1, proj)

	for y := range fb.Height {
		for x := range fb.Width {
			if fb.Pixels[y*fb.Width+x] != 0 {
				t.Errorf("behind-camera triangle wrote pixel (%d,%d)", x, y)
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

	p0 := Vec3{X: 0, Y: 0.8, Z: 0}
	p1 := Vec3{X: -0.8, Y: -0.6, Z: 0}
	p2 := Vec3{X: 0.8, Y: -0.6, Z: 0}

	aspect := float64(width) / float64(height)
	proj := Perspective(math.Pi/4, aspect, 0.1, 100)
	view := LookAt(Vec3{X: 0, Y: 0, Z: 3}, Vec3{}, Vec3{X: 0, Y: 1, Z: 0})
	model := RotateY(0.5)
	mvp := proj.Mul(view).Mul(model)

	b.ResetTimer()
	b.ReportAllocs()
	for range b.N {
		fb.Clear()
		fb.DrawTriangle(p0, p1, p2, 1, mvp)
	}
}

func BenchmarkDrawTriangle640x480(b *testing.B)   { benchmarkDrawTriangle(b, 640, 480) }
func BenchmarkDrawTriangle800x600(b *testing.B)   { benchmarkDrawTriangle(b, 800, 600) }
func BenchmarkDrawTriangle1920x1080(b *testing.B) { benchmarkDrawTriangle(b, 1920, 1080) }
