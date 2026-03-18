package solver

import (
	"errors"
	"math"

	"experiment/puzzle"
)

// ErrNoSolution indicates no solution was found within the depth limit.
var ErrNoSolution = errors.New("no solution found within depth limit")

const foundSentinel = -1

// IDA runs iterative-deepening A* on the puzzle for the given phase.
// The puzzle is left in its original state after the call.
// Returns the sequence of moves that satisfies the phase, or an error.
func IDA(p *puzzle.Puzzle, phase Phase, maxDepth int) ([]Move, error) {
	if phase.IsSatisfied(p) {
		return nil, nil
	}

	s := &idaState{
		puzzle: p,
		phase:  phase,
		moves:  phase.Moves(p),
		path:   make([]Move, 0, maxDepth),
	}

	bound := phase.Heuristic(p)
	for bound <= maxDepth {
		logger.Info("IDA* iteration", "phase", phase.Name(), "bound", bound)
		t := s.search(0, bound)
		if t == foundSentinel {
			logger.Info("IDA* found solution", "phase", phase.Name(), "depth", len(s.path))
			result := make([]Move, len(s.path))
			copy(result, s.path)
			// Restore puzzle to original state.
			for i := len(s.path) - 1; i >= 0; i-- {
				s.path[i].Unapply(p)
			}
			return result, nil
		}
		if t == math.MaxInt {
			logger.Info("IDA* exhausted search space", "phase", phase.Name(), "bound", bound)
			return nil, ErrNoSolution
		}
		bound = t
	}
	logger.Info("IDA* exceeded max depth", "phase", phase.Name(), "maxDepth", maxDepth)
	return nil, ErrNoSolution
}

type idaState struct {
	puzzle *puzzle.Puzzle
	phase  Phase
	moves  []Move
	path   []Move
}

func (s *idaState) search(g, bound int) int {
	h := s.phase.Heuristic(s.puzzle)
	f := g + h
	if f > bound {
		return f
	}
	if s.phase.IsSatisfied(s.puzzle) {
		return foundSentinel
	}

	min := math.MaxInt
	for _, m := range s.moves {
		if len(s.path) > 0 {
			last := s.path[len(s.path)-1]
			// Same axis, same slice: redundant (could be combined).
			if m.Axis == last.Axis && m.Slice == last.Slice {
				continue
			}
			// Same axis: enforce canonical slice ordering.
			if m.Axis == last.Axis && m.Slice < last.Slice {
				continue
			}
		}

		m.Apply(s.puzzle)
		s.path = append(s.path, m)

		t := s.search(g+1, bound)
		if t == foundSentinel {
			return foundSentinel
		}
		if t < min {
			min = t
		}

		s.path = s.path[:len(s.path)-1]
		m.Unapply(s.puzzle)
	}
	return min
}
