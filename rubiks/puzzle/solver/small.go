package solver

import "experiment/puzzle"

// SmallPhase solves any puzzle where all dimensions are <= 3.
type SmallPhase struct {
	moves []Move
	h     *heuristicState
}

// NewSmallPhase creates a phase for solving small puzzles (all dims <= 3).
func NewSmallPhase(p *puzzle.Puzzle) *SmallPhase {
	moves := GenerateMoves(p)
	return &SmallPhase{
		moves: moves,
		h:     newHeuristicState(p, moves),
	}
}

func (s *SmallPhase) Name() string { return "small" }

func (s *SmallPhase) IsSatisfied(p *puzzle.Puzzle) bool {
	return IsSolved(p)
}

func (s *SmallPhase) Heuristic(p *puzzle.Puzzle) int {
	return s.h.Heuristic(p)
}

func (s *SmallPhase) Moves(_ *puzzle.Puzzle) []Move {
	return s.moves
}
