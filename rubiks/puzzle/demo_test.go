package puzzle_test

import (
	"testing"

	"experiment/puzzle"
	"experiment/puzzle/solver"
)

func TestDemoShuffleCount(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	ds := puzzle.NewDemoState(p)

	n := len(ds.Moves)
	if n < puzzle.MinShuffleMoves || n > puzzle.MaxShuffleMoves {
		t.Errorf("shuffle move count = %d, want [%d, %d]", n, puzzle.MinShuffleMoves, puzzle.MaxShuffleMoves)
	}
}

func TestDemoFullCycle(t *testing.T) {
	p := puzzle.NewPuzzle(2, 2, 2)

	// Use a small known scramble (3 moves) so the solver finishes quickly.
	scramble := []puzzle.SliceMove{
		{Axis: puzzle.AxisX, Slice: 1, Turns: 1},
		{Axis: puzzle.AxisY, Slice: 1, Turns: -1},
		{Axis: puzzle.AxisZ, Slice: 1, Turns: 1},
	}

	ds := puzzle.NewDemoState(p)
	// Override the random shuffle moves with our small scramble.
	ds.Moves = scramble

	if ds.Phase != puzzle.PhaseShuffling {
		t.Fatalf("initial phase = %d, want PhaseShuffling", ds.Phase)
	}

	// Drain all shuffle animations
	for ds.Phase == puzzle.PhaseShuffling {
		anim := ds.NextAnimation(p, 0)
		if anim == nil {
			break
		}
		anim.ApplyTurn(p)
		ds.AnimationDone()
	}

	if ds.Phase != puzzle.PhaseShufflePause {
		t.Fatalf("after shuffle, phase = %d, want PhaseShufflePause", ds.Phase)
	}

	// Tick through the pause
	ds.NextAnimation(p, puzzle.ShufflePauseDuration+1)

	if !ds.NeedsSolve() {
		t.Fatal("expected NeedsSolve() = true after shuffle pause")
	}

	// Solve
	moves, err := solver.Solve(p)
	if err != nil {
		t.Fatalf("solver error: %v", err)
	}

	sliceMoves := make([]puzzle.SliceMove, len(moves))
	for i, m := range moves {
		sliceMoves[i] = puzzle.SliceMove{Axis: m.Axis, Slice: m.Slice, Turns: m.Turns}
	}
	ds.SetSolveMoves(sliceMoves)

	if ds.Phase != puzzle.PhaseSolving {
		t.Fatalf("after SetSolveMoves, phase = %d, want PhaseSolving", ds.Phase)
	}

	// Drain all solve animations
	for ds.Phase == puzzle.PhaseSolving {
		anim := ds.NextAnimation(p, 0)
		if anim == nil {
			break
		}
		anim.ApplyTurn(p)
		ds.AnimationDone()
	}

	if ds.Phase != puzzle.PhaseSolvePause {
		t.Fatalf("after solve, phase = %d, want PhaseSolvePause", ds.Phase)
	}

	if !solver.IsSolved(p) {
		t.Error("puzzle not solved after solve phase")
	}

	// Tick through solve pause
	ds.NextAnimation(p, puzzle.SolvePauseDuration+1)

	if ds.Phase != puzzle.PhaseShuffling {
		t.Fatalf("after solve pause, phase = %d, want PhaseShuffling", ds.Phase)
	}
}

func TestGenerateTrianglesSplitSlice(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)

	stationary, rotating := puzzle.GenerateTrianglesSplitSlice(p, puzzle.AxisY, 1)

	if len(rotating) == 0 {
		t.Error("no rotating triangles for middle slice")
	}
	if len(stationary) == 0 {
		t.Error("no stationary triangles for middle slice")
	}

	// All triangles should be accounted for
	all := puzzle.GenerateTriangles(p, 0.05)
	total := len(stationary) + len(rotating)
	if total != len(all) {
		t.Errorf("split total = %d, want %d", total, len(all))
	}
}
