package solver

import "experiment/puzzle"

// EdgesPhase pairs edge strips for axes with dimension > 3.
// TODO: implement for puzzles with dimensions > 3.
type EdgesPhase struct {
	moves []Move
}

// NewEdgesPhase creates an edges phase for the given puzzle.
func NewEdgesPhase(p *puzzle.Puzzle) *EdgesPhase {
	return &EdgesPhase{moves: GenerateMoves(p)}
}

func (*EdgesPhase) Name() string                   { return "edges" }
func (*EdgesPhase) IsSatisfied(_ *puzzle.Puzzle) bool { return true }
func (*EdgesPhase) Heuristic(_ *puzzle.Puzzle) int    { return 0 }
func (e *EdgesPhase) Moves(_ *puzzle.Puzzle) []Move   { return e.moves }
