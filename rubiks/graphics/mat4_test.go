package graphics

import (
	"math"
	"testing"
)

func TestIdentityMul(t *testing.T) {
	id := Identity()
	m := Translate(1, 2, 3)
	result := id.Mul(m)
	if result != m {
		t.Errorf("Identity * M != M\ngot  %v\nwant %v", result, m)
	}
	result = m.Mul(id)
	if result != m {
		t.Errorf("M * Identity != M\ngot  %v\nwant %v", result, m)
	}
}

func TestTranslatePoint(t *testing.T) {
	m := Translate(10, 20, 30)
	p := m.TransformPoint(Vec3{1, 2, 3})
	assertVec3Near(t, p, Vec3{11, 22, 33})
}

func TestRotateY90(t *testing.T) {
	m := RotateY(math.Pi / 2)
	// Rotating (1,0,0) 90 degrees around Y should give approximately (0,0,-1)
	p := m.TransformPoint(Vec3{1, 0, 0})
	assertVec3Near(t, p, Vec3{0, 0, -1})
}

func TestPerspectiveNearPlane(t *testing.T) {
	near := 0.1
	far := 100.0
	m := Perspective(math.Pi/2, 1, near, far)
	// A point on the near plane at center should map to z=-1 in NDC
	p := m.TransformPoint(Vec3{0, 0, -near})
	if math.Abs(p.Z-(-1)) > 1e-6 {
		t.Errorf("near plane z = %f, want -1", p.Z)
	}
}

func TestPerspectiveFarPlane(t *testing.T) {
	near := 0.1
	far := 100.0
	m := Perspective(math.Pi/2, 1, near, far)
	p := m.TransformPoint(Vec3{0, 0, -far})
	if math.Abs(p.Z-1) > 1e-6 {
		t.Errorf("far plane z = %f, want 1", p.Z)
	}
}

func TestRotateX90(t *testing.T) {
	m := RotateX(math.Pi / 2)
	// Rotating (0,1,0) 90 degrees around X should give approximately (0,0,1)
	p := m.TransformPoint(Vec3{0, 1, 0})
	assertVec3Near(t, p, Vec3{0, 0, 1})
}

func TestRotateZ90(t *testing.T) {
	m := RotateZ(math.Pi / 2)
	// Rotating (1,0,0) 90 degrees around Z should give approximately (0,1,0)
	p := m.TransformPoint(Vec3{1, 0, 0})
	assertVec3Near(t, p, Vec3{0, 1, 0})
}

// --- Benchmarks ---

func BenchmarkMVPBuild(b *testing.B) {
	b.ReportAllocs()
	for range b.N {
		proj := Perspective(math.Pi/4, 800.0/600.0, 0.1, 100)
		view := LookAt(Vec3{X: 0, Y: 0, Z: 3}, Vec3{}, Vec3{X: 0, Y: 1, Z: 0})
		model := RotateY(0.5)
		mvp := proj.Mul(view).Mul(model)
		_ = mvp
	}
}

func TestLookAtIdentityView(t *testing.T) {
	// Looking down -Z from origin: point at (0,0,-5) should stay centered.
	// OpenGL convention: objects in front of camera have negative z in view space.
	m := LookAt(Vec3{0, 0, 0}, Vec3{0, 0, -1}, Vec3{0, 1, 0})
	p := m.TransformPoint(Vec3{0, 0, -5})
	if math.Abs(p.X) > epsilon || math.Abs(p.Y) > epsilon {
		t.Errorf("center point moved off-center: %v", p)
	}
	if p.Z >= 0 {
		t.Errorf("expected negative z in view space (in front of camera), got %f", p.Z)
	}
}
