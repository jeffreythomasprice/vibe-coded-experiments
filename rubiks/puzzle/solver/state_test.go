package solver

import (
	"testing"

	"experiment/puzzle"
)

func TestIsSolvedFreshPuzzle(t *testing.T) {
	tests := []struct {
		name       string
		sx, sy, sz int
	}{
		{"2x2x2", 2, 2, 2},
		{"3x3x3", 3, 3, 3},
		{"2x3x3", 2, 3, 3},
		{"2x3x4", 2, 3, 4},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p := puzzle.NewPuzzle(tt.sx, tt.sy, tt.sz)
			if !IsSolved(p) {
				t.Error("fresh puzzle should be solved")
			}
		})
	}
}

func TestIsSolvedAfterMove(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	p.SliceTurn(puzzle.AxisY, 2, true)
	if IsSolved(p) {
		t.Error("puzzle should not be solved after a single move")
	}
}

func TestCountMisplacedSolved(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	if got := CountMisplaced(p); got != 0 {
		t.Errorf("solved puzzle has %d misplaced facelets, want 0", got)
	}
}

func TestCountMisplacedAfterMove(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	p.SliceTurn(puzzle.AxisY, 2, true)
	if got := CountMisplaced(p); got == 0 {
		t.Error("puzzle should have misplaced facelets after a move")
	}
}
