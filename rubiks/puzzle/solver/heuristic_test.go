package solver

import (
	"testing"

	"experiment/puzzle"
)

func TestColorSignature(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	// Corner at (0,0,0) should have NegX=Orange, NegY=Yellow, NegZ=Green.
	var corner puzzle.Cubie
	for _, c := range p.Cubies {
		if c.Pos == [3]int{0, 0, 0} {
			corner = c
			break
		}
	}
	sig := cubieColorSig(corner)
	// Orange=3, Yellow=6, Green=5 → sorted: 3, 5, 6.
	want := colorSig{puzzle.Orange, puzzle.Green, puzzle.Yellow}
	if sig != want {
		t.Errorf("corner (0,0,0) sig = %v, want %v", sig, want)
	}
}

func TestHomePositionSolved(t *testing.T) {
	for _, size := range [][3]int{{2, 2, 2}, {3, 3, 3}, {2, 3, 4}} {
		p := puzzle.NewPuzzle(size[0], size[1], size[2])
		moves := GenerateMoves(p)
		h := newHeuristicState(p, moves)

		for _, c := range p.Cubies {
			sig := cubieColorSig(c)
			homeList := h.homes[sig]
			found := false
			for _, home := range homeList {
				if home == c.Pos {
					found = true
					break
				}
			}
			if !found {
				t.Errorf("size %v: cubie at %v (sig %v) not found in homes %v",
					size, c.Pos, sig, homeList)
			}
		}
	}
}

func TestHeuristicSolvedIsZero(t *testing.T) {
	for _, size := range [][3]int{{2, 2, 2}, {3, 3, 3}, {2, 3, 4}} {
		p := puzzle.NewPuzzle(size[0], size[1], size[2])
		moves := GenerateMoves(p)
		h := newHeuristicState(p, moves)
		got := h.Heuristic(p)
		if got != 0 {
			t.Errorf("size %v: solved heuristic = %d, want 0", size, got)
		}
	}
}

func TestHeuristicAfterOneMove(t *testing.T) {
	for _, size := range [][3]int{{2, 2, 2}, {3, 3, 3}} {
		p := puzzle.NewPuzzle(size[0], size[1], size[2])
		moves := GenerateMoves(p)
		h := newHeuristicState(p, moves)

		for _, m := range moves {
			m.Apply(p)
			got := h.Heuristic(p)
			if got < 1 {
				t.Errorf("size %v, move %v: h = %d, want >= 1", size, m, got)
			}
			m.Unapply(p)
		}
	}
}

func TestHeuristicAdmissible(t *testing.T) {
	for _, size := range [][3]int{{2, 2, 2}, {3, 3, 3}} {
		p := puzzle.NewPuzzle(size[0], size[1], size[2])
		moves := GenerateMoves(p)
		h := newHeuristicState(p, moves)

		for depth := 1; depth <= 5; depth++ {
			scrambled := p.Clone()
			for i := 0; i < depth; i++ {
				moves[i%len(moves)].Apply(scrambled)
			}
			got := h.Heuristic(scrambled)
			if got > depth {
				t.Errorf("size %v, depth %d: h = %d > depth (not admissible)", size, depth, got)
			}
		}
	}
}

func TestHeuristicStrongerThanOld(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	moves := GenerateMoves(p)
	h := newHeuristicState(p, moves)

	// Build old-style maxAffected for comparison.
	solved := puzzle.NewPuzzle(3, 3, 3)
	maxAffected := 1
	for _, m := range moves {
		test := solved.Clone()
		m.Apply(test)
		changed := CountMisplaced(test)
		if changed > maxAffected {
			maxAffected = changed
		}
	}

	// Apply a 5-move scramble.
	scramble := []Move{
		{Axis: puzzle.AxisX, Slice: 2, Turns: 1},
		{Axis: puzzle.AxisY, Slice: 0, Turns: -1},
		{Axis: puzzle.AxisZ, Slice: 2, Turns: 1},
		{Axis: puzzle.AxisX, Slice: 0, Turns: 2},
		{Axis: puzzle.AxisY, Slice: 2, Turns: 1},
	}
	for _, m := range scramble {
		m.Apply(p)
	}

	newH := h.Heuristic(p)
	misplaced := CountMisplaced(p)
	oldH := (misplaced + maxAffected - 1) / maxAffected

	if newH < oldH {
		t.Errorf("new heuristic %d < old heuristic %d", newH, oldH)
	}
}
