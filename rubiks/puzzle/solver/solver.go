package solver

import (
	"fmt"
	"time"

	"experiment/puzzle"
)

// maxSmallDepth is the depth limit for solving small puzzles (all dims <= 3).
// God's number for 3x3x3 is 20 in half-turn metric.
const maxSmallDepth = 20

// Solve finds a sequence of moves to solve the puzzle.
// The puzzle is left in its original state after the call.
func Solve(p *puzzle.Puzzle) ([]Move, error) {
	solveStart := time.Now()
	if IsSolved(p) {
		logger.Info("puzzle already solved")
		return nil, nil
	}

	phases := buildPhases(p)
	var allMoves []Move
	for _, phase := range phases {
		if phase.IsSatisfied(p) {
			logger.Info("phase already satisfied, skipping", "phase", phase.Name())
			continue
		}
		logger.Info("starting phase", "phase", phase.Name())
		phaseStart := time.Now()
		moves, err := IDA(p, phase, maxSmallDepth)
		if err != nil {
			return nil, fmt.Errorf("phase %s: %w", phase.Name(), err)
		}
		logger.Info("phase complete", "phase", phase.Name(), "moves", len(moves), "elapsed", time.Since(phaseStart))
		for _, m := range moves {
			m.Apply(p)
		}
		allMoves = append(allMoves, moves...)
	}
	logger.Info("solve complete", "total_moves", len(allMoves), "elapsed", time.Since(solveStart))

	// Restore puzzle to original state.
	for i := len(allMoves) - 1; i >= 0; i-- {
		allMoves[i].Unapply(p)
	}
	return allMoves, nil
}

func buildPhases(p *puzzle.Puzzle) []Phase {
	allSmall := true
	for _, d := range p.Size {
		if d > 3 {
			allSmall = false
			break
		}
	}

	if allSmall {
		return []Phase{NewSmallPhase(p)}
	}

	return []Phase{
		NewCentersPhase(p),
		NewEdgesPhase(p),
		NewParityPhase(p),
		NewSmallPhase(p),
	}
}
