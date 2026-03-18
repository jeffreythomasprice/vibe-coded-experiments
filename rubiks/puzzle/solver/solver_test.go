package solver

import (
	"testing"

	"experiment/puzzle"
)

func TestSolve2x2x2(t *testing.T) {
	p := puzzle.NewPuzzle(2, 2, 2)
	scramble := []Move{
		{Axis: puzzle.AxisX, Slice: 1, Turns: 1},
		{Axis: puzzle.AxisY, Slice: 1, Turns: -1},
		{Axis: puzzle.AxisZ, Slice: 1, Turns: 1},
	}
	for _, m := range scramble {
		m.Apply(p)
	}

	solution, err := Solve(p)
	if err != nil {
		t.Fatal(err)
	}
	for _, m := range solution {
		m.Apply(p)
	}
	if !IsSolved(p) {
		t.Errorf("puzzle not solved after %d-move solution", len(solution))
	}
}

func TestSolve3x3x3(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	scramble := []Move{
		{Axis: puzzle.AxisY, Slice: 2, Turns: 1},
		{Axis: puzzle.AxisX, Slice: 2, Turns: -1},
		{Axis: puzzle.AxisZ, Slice: 0, Turns: 1},
		{Axis: puzzle.AxisY, Slice: 0, Turns: 2},
	}
	for _, m := range scramble {
		m.Apply(p)
	}

	solution, err := Solve(p)
	if err != nil {
		t.Fatal(err)
	}
	for _, m := range solution {
		m.Apply(p)
	}
	if !IsSolved(p) {
		t.Errorf("puzzle not solved after %d-move solution", len(solution))
	}
}

func TestSolve2x2x3(t *testing.T) {
	p := puzzle.NewPuzzle(2, 2, 3)
	// AxisZ: CanTurn90 (X==Y=2), AxisX/Y: 180° only
	scramble := []Move{
		{Axis: puzzle.AxisZ, Slice: 2, Turns: 1},
		{Axis: puzzle.AxisX, Slice: 1, Turns: 2},
		{Axis: puzzle.AxisZ, Slice: 0, Turns: -1},
	}
	for _, m := range scramble {
		m.Apply(p)
	}

	solution, err := Solve(p)
	if err != nil {
		t.Fatal(err)
	}
	for _, m := range solution {
		m.Apply(p)
	}
	if !IsSolved(p) {
		t.Errorf("puzzle not solved after %d-move solution", len(solution))
	}
}

func TestSolve2x3x3(t *testing.T) {
	p := puzzle.NewPuzzle(2, 3, 3)
	// AxisX: CanTurn90 (Y==Z=3), AxisY/Z: 180° only
	scramble := []Move{
		{Axis: puzzle.AxisX, Slice: 1, Turns: 1},
		{Axis: puzzle.AxisY, Slice: 2, Turns: 2},
		{Axis: puzzle.AxisX, Slice: 0, Turns: -1},
	}
	for _, m := range scramble {
		m.Apply(p)
	}

	solution, err := Solve(p)
	if err != nil {
		t.Fatal(err)
	}
	for _, m := range solution {
		m.Apply(p)
	}
	if !IsSolved(p) {
		t.Errorf("puzzle not solved after %d-move solution", len(solution))
	}
}

func TestSolveAlreadySolved(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	solution, err := Solve(p)
	if err != nil {
		t.Fatal(err)
	}
	if len(solution) != 0 {
		t.Errorf("expected empty solution for solved puzzle, got %d moves", len(solution))
	}
}

func TestSolveDeeperScramble(t *testing.T) {
	p := puzzle.NewPuzzle(3, 3, 3)
	scramble := []Move{
		{Axis: puzzle.AxisX, Slice: 2, Turns: 1},
		{Axis: puzzle.AxisY, Slice: 0, Turns: -1},
		{Axis: puzzle.AxisZ, Slice: 2, Turns: 1},
		{Axis: puzzle.AxisX, Slice: 0, Turns: 2},
		{Axis: puzzle.AxisY, Slice: 2, Turns: 1},
		{Axis: puzzle.AxisZ, Slice: 0, Turns: -1},
	}
	for _, m := range scramble {
		m.Apply(p)
	}

	solution, err := Solve(p)
	if err != nil {
		t.Fatal(err)
	}
	for _, m := range solution {
		m.Apply(p)
	}
	if !IsSolved(p) {
		t.Errorf("puzzle not solved after %d-move solution", len(solution))
	}
}

func TestMultiPhaseChaining(t *testing.T) {
	// 3x3x3 goes through buildPhases → SmallPhase only (stubs skipped).
	p := puzzle.NewPuzzle(3, 3, 3)
	scramble := []Move{
		{Axis: puzzle.AxisY, Slice: 2, Turns: 1},
		{Axis: puzzle.AxisX, Slice: 0, Turns: -1},
		{Axis: puzzle.AxisZ, Slice: 2, Turns: 2},
	}
	for _, m := range scramble {
		m.Apply(p)
	}

	solution, err := Solve(p)
	if err != nil {
		t.Fatal(err)
	}
	for _, m := range solution {
		m.Apply(p)
	}
	if !IsSolved(p) {
		t.Errorf("puzzle not solved after %d-move solution", len(solution))
	}
}

func TestSolvePreservesState(t *testing.T) {
	p := puzzle.NewPuzzle(2, 2, 2)
	scramble := []Move{
		{Axis: puzzle.AxisX, Slice: 1, Turns: 1},
		{Axis: puzzle.AxisY, Slice: 1, Turns: -1},
	}
	for _, m := range scramble {
		m.Apply(p)
	}
	snapshot := p.Clone()

	_, err := Solve(p)
	if err != nil {
		t.Fatal(err)
	}

	// Puzzle should be unchanged after Solve.
	if !cubiesEqual(snapshot.Cubies, p.Cubies) {
		t.Error("Solve modified the puzzle state")
	}
}
