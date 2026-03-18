package solver

import "experiment/puzzle"

// Phase defines a solving phase for IDA*.
type Phase interface {
	Name() string
	IsSatisfied(p *puzzle.Puzzle) bool
	Heuristic(p *puzzle.Puzzle) int // admissible lower bound on moves remaining
	Moves(p *puzzle.Puzzle) []Move  // legal moves for this phase
}
