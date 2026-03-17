package puzzle

import (
	"experiment/graphics"
	"testing"
)

func TestTriangleCount2x2x2(t *testing.T) {
	p := NewPuzzle(2, 2, 2)
	tris := GenerateTriangles(p, 0.05)
	// 8 cubies, each with 3 exposed faces, 2 triangles per face = 48
	if len(tris) != 48 {
		t.Errorf("2x2x2 triangle count = %d, want 48", len(tris))
	}
}

func TestTriangleCount3x3x3(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	tris := GenerateTriangles(p, 0.05)
	// 6 faces × 9 stickers × 2 triangles = 108
	if len(tris) != 108 {
		t.Errorf("3x3x3 triangle count = %d, want 108", len(tris))
	}
}

func TestVertexBounds(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	tris := GenerateTriangles(p, 0.05)
	maxCoord := 1.5 + 0.5 // center offset + half size
	for i, tri := range tris {
		for _, v := range [3]struct{ x, y, z float64 }{
			{tri.P0.X, tri.P0.Y, tri.P0.Z},
			{tri.P1.X, tri.P1.Y, tri.P1.Z},
			{tri.P2.X, tri.P2.Y, tri.P2.Z},
		} {
			if v.x < -maxCoord || v.x > maxCoord ||
				v.y < -maxCoord || v.y > maxCoord ||
				v.z < -maxCoord || v.z > maxCoord {
				t.Errorf("triangle %d vertex out of bounds: (%f, %f, %f)", i, v.x, v.y, v.z)
			}
		}
	}
}

func TestWindingOrder(t *testing.T) {
	// For a 1x1x1 puzzle, verify that every triangle's face normal
	// (computed via cross product of edges) points outward.
	p := NewPuzzle(1, 1, 1)
	tris := GenerateTriangles(p, 0.05)
	if len(tris) != 12 {
		t.Fatalf("expected 12 triangles for a 1x1x1, got %d", len(tris))
	}

	// Expected outward direction per face color.
	expectedDir := map[byte]graphics.Vec3{
		byte(Red):    {X: 1, Y: 0, Z: 0},  // PosX
		byte(Orange): {X: -1, Y: 0, Z: 0}, // NegX
		byte(White):  {X: 0, Y: 1, Z: 0},  // PosY
		byte(Yellow): {X: 0, Y: -1, Z: 0}, // NegY
		byte(Blue):   {X: 0, Y: 0, Z: 1},  // PosZ
		byte(Green):  {X: 0, Y: 0, Z: -1}, // NegZ
	}

	for i, tri := range tris {
		e1 := tri.P1.Sub(tri.P0)
		e2 := tri.P2.Sub(tri.P0)
		normal := e1.Cross(e2)

		dir, ok := expectedDir[tri.ColorIdx]
		if !ok {
			t.Errorf("triangle %d: unexpected color index %d", i, tri.ColorIdx)
			continue
		}
		dot := normal.Dot(dir)
		if dot <= 0 {
			t.Errorf("triangle %d (color %d): normal dot expected direction = %f, want > 0", i, tri.ColorIdx, dot)
		}
	}
}

func TestSplitPartitioning(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	stationary, rotating := GenerateTrianglesSplit(p, AxisY, 0)

	total := GenerateTriangles(p, 0.05)
	if len(stationary)+len(rotating) != len(total) {
		t.Errorf("split count %d + %d = %d, want %d",
			len(stationary), len(rotating),
			len(stationary)+len(rotating), len(total))
	}

	if len(stationary) == 0 || len(rotating) == 0 {
		t.Error("split should produce non-empty groups for layer 0")
	}
}
