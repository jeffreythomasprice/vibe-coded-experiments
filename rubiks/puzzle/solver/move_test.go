package solver

import (
	"testing"

	"experiment/puzzle"
)

func TestApplyUnapplyRestores(t *testing.T) {
	tests := []struct {
		name       string
		sx, sy, sz int
	}{
		{"3x3x3", 3, 3, 3},
		{"2x2x2", 2, 2, 2},
		{"2x3x3", 2, 3, 3},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p := puzzle.NewPuzzle(tt.sx, tt.sy, tt.sz)
			moves := GenerateMoves(p)
			for _, m := range moves {
				original := p.Clone()
				m.Apply(p)
				m.Unapply(p)
				if !cubiesEqual(original.Cubies, p.Cubies) {
					t.Errorf("move %+v: apply+unapply did not restore puzzle", m)
				}
			}
		})
	}
}

func TestInverseApplyRestores(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	moves := GenerateMoves(p)
	for _, m := range moves {
		original := p.Clone()
		m.Apply(p)
		m.Inverse().Apply(p)
		if !cubiesEqual(original.Cubies, p.Cubies) {
			t.Errorf("move %+v: apply+inverse.apply did not restore puzzle", m)
		}
	}
}

func TestGenerateMovesCount(t *testing.T) {
	tests := []struct {
		name       string
		sx, sy, sz int
		want       int
	}{
		// 3x3x3: 3 axes × 3 slices × 3 turns (CW, CCW, half) = 27
		{"3x3x3", 3, 3, 3, 27},
		// 2x2x2: 3 axes × 2 slices × 3 turns = 18
		{"2x2x2", 2, 2, 2, 18},
		// 2x3x4: no axis has CanTurn90
		// AxisX: 2 slices × 1 = 2, AxisY: 3 slices × 1 = 3, AxisZ: 4 slices × 1 = 4 → 9
		{"2x3x4", 2, 3, 4, 9},
		// 2x3x3: AxisX has CanTurn90 (Y==Z=3), AxisY/Z don't (X≠Z, X≠Y)
		// AxisX: 2×3=6, AxisY: 3×1=3, AxisZ: 3×1=3 → 12
		{"2x3x3", 2, 3, 3, 12},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p := puzzle.NewPuzzle(tt.sx, tt.sy, tt.sz)
			moves := GenerateMoves(p)
			if len(moves) != tt.want {
				t.Errorf("got %d moves, want %d", len(moves), tt.want)
			}
		})
	}
}

// cubiesEqual compares two cubie slices ignoring order.
func cubiesEqual(a, b []puzzle.Cubie) bool {
	if len(a) != len(b) {
		return false
	}
	for _, ca := range a {
		found := false
		for _, cb := range b {
			if ca.Pos == cb.Pos && ca.Colors == cb.Colors {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}
	return true
}
