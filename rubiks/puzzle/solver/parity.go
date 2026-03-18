package solver

import "experiment/puzzle"

// ParityPhase detects and fixes parity states caused by even dimensions.
// TODO: implement for puzzles with even dimensions > 2.
type ParityPhase struct {
	moves []Move
}

// NewParityPhase creates a parity phase for the given puzzle.
func NewParityPhase(p *puzzle.Puzzle) *ParityPhase {
	return &ParityPhase{moves: GenerateMoves(p)}
}

func (*ParityPhase) Name() string                   { return "parity" }
func (*ParityPhase) IsSatisfied(_ *puzzle.Puzzle) bool { return true }
func (*ParityPhase) Heuristic(_ *puzzle.Puzzle) int    { return 0 }
func (pa *ParityPhase) Moves(_ *puzzle.Puzzle) []Move  { return pa.moves }
