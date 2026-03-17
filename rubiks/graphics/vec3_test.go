package graphics

import (
	"math"
	"testing"
)

const epsilon = 1e-9

func assertVec3Near(t *testing.T, got, want Vec3) {
	t.Helper()
	if math.Abs(got.X-want.X) > epsilon || math.Abs(got.Y-want.Y) > epsilon || math.Abs(got.Z-want.Z) > epsilon {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestVec3Add(t *testing.T) {
	assertVec3Near(t, Vec3{1, 2, 3}.Add(Vec3{4, 5, 6}), Vec3{5, 7, 9})
}

func TestVec3Sub(t *testing.T) {
	assertVec3Near(t, Vec3{4, 5, 6}.Sub(Vec3{1, 2, 3}), Vec3{3, 3, 3})
}

func TestVec3Scale(t *testing.T) {
	assertVec3Near(t, Vec3{1, 2, 3}.Scale(2), Vec3{2, 4, 6})
}

func TestVec3DotOrthogonal(t *testing.T) {
	d := Vec3{1, 0, 0}.Dot(Vec3{0, 1, 0})
	if math.Abs(d) > epsilon {
		t.Errorf("dot of orthogonal vectors = %f, want 0", d)
	}
}

func TestVec3DotParallel(t *testing.T) {
	d := Vec3{3, 0, 0}.Dot(Vec3{4, 0, 0})
	if math.Abs(d-12) > epsilon {
		t.Errorf("got %f, want 12", d)
	}
}

func TestVec3Cross(t *testing.T) {
	// x cross y = z
	assertVec3Near(t, Vec3{1, 0, 0}.Cross(Vec3{0, 1, 0}), Vec3{0, 0, 1})
	// y cross x = -z
	assertVec3Near(t, Vec3{0, 1, 0}.Cross(Vec3{1, 0, 0}), Vec3{0, 0, -1})
}

func TestVec3Len(t *testing.T) {
	l := Vec3{3, 4, 0}.Len()
	if math.Abs(l-5) > epsilon {
		t.Errorf("got %f, want 5", l)
	}
}

func TestVec3Normalize(t *testing.T) {
	n := Vec3{3, 0, 0}.Normalize()
	assertVec3Near(t, n, Vec3{1, 0, 0})
}

func TestVec3NormalizeZero(t *testing.T) {
	n := Vec3{}.Normalize()
	assertVec3Near(t, n, Vec3{})
}
