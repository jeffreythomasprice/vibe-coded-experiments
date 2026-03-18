package solver

import "experiment/puzzle"

// CentersPhase solves center stickers for axes with dimension > 3.
// TODO: implement for puzzles with dimensions > 3.
type CentersPhase struct {
	moves []Move
}

// NewCentersPhase creates a centers phase for the given puzzle.
func NewCentersPhase(p *puzzle.Puzzle) *CentersPhase {
	return &CentersPhase{moves: GenerateMoves(p)}
}

func (*CentersPhase) Name() string                   { return "centers" }
func (*CentersPhase) IsSatisfied(_ *puzzle.Puzzle) bool { return true }
func (*CentersPhase) Heuristic(_ *puzzle.Puzzle) int    { return 0 }
func (c *CentersPhase) Moves(_ *puzzle.Puzzle) []Move   { return c.moves }
