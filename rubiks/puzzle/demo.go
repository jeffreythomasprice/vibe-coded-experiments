package puzzle

import (
	"log/slog"
	"math/rand/v2"
)

// DemoPhase represents a phase in the shuffle-then-solve demo loop.
type DemoPhase int

const (
	PhaseShuffling    DemoPhase = iota
	PhaseShufflePause           // pause after shuffle, before solve
	PhaseSolving
	PhaseSolvePause // pause after solve, before next shuffle
)

const (
	ShufflePauseDuration = 3.0 // seconds
	SolvePauseDuration   = 3.0
	ShuffleTurnDuration  = 0.5 // seconds per shuffle move
	SolveTurnDuration    = 1.0 // seconds per solve move
	MinShuffleMoves      = 20
	MaxShuffleMoves      = 30
)

// SliceMove describes a single-slice turn for the demo state machine.
type SliceMove struct {
	Axis  Axis
	Slice int
	Turns int // 1, -1, or 2
}

// DemoState drives the shuffle→pause→solve→pause loop.
type DemoState struct {
	Phase          DemoPhase
	Moves          []SliceMove
	MoveIndex      int
	PauseRemaining float64
	animActive     bool // true while an animation is playing
	needsSolve     bool // signals main.go to call solver
	logger         *slog.Logger
}

// NewDemoState creates a DemoState that starts in the shuffling phase.
func NewDemoState(p *Puzzle) *DemoState {
	ds := &DemoState{Phase: PhaseShuffling}
	ds.Moves = generateShuffleMoves(p)
	return ds
}

// SetLogger configures logging for state transitions.
func (ds *DemoState) SetLogger(l *slog.Logger) {
	ds.logger = l
}

func (ds *DemoState) log() *slog.Logger {
	if ds.logger != nil {
		return ds.logger
	}
	return slog.Default()
}

// NeedsSolve returns true when the state machine needs solver results.
func (ds *DemoState) NeedsSolve() bool {
	return ds.needsSolve
}

// SetSolveMoves provides the solver's move list for the solving phase.
func (ds *DemoState) SetSolveMoves(moves []SliceMove) {
	ds.needsSolve = false
	ds.Moves = moves
	ds.MoveIndex = 0
	if len(moves) == 0 {
		// Already solved — skip to solve pause
		ds.Phase = PhaseSolvePause
		ds.PauseRemaining = SolvePauseDuration
	} else {
		ds.log().Info("start of playing solution", "moves", len(moves))
		ds.Phase = PhaseSolving
	}
}

// NextAnimation returns the next animation to play, or nil if the state
// machine is pausing. dt is the elapsed time since the last call.
func (ds *DemoState) NextAnimation(p *Puzzle, dt float64) *Animation {
	if ds.animActive {
		return nil
	}

	switch ds.Phase {
	case PhaseShuffling:
		return ds.nextShuffleAnim(p)
	case PhaseShufflePause:
		ds.PauseRemaining -= dt
		if ds.PauseRemaining <= 0 {
			ds.needsSolve = true
			// Stay in ShufflePause until SetSolveMoves is called
		}
		return nil
	case PhaseSolving:
		return ds.nextSolveAnim(p)
	case PhaseSolvePause:
		ds.PauseRemaining -= dt
		if ds.PauseRemaining <= 0 {
			ds.Phase = PhaseShuffling
			ds.Moves = generateShuffleMoves(p)
			ds.MoveIndex = 0
			return ds.nextShuffleAnim(p) // nextShuffleAnim logs "start of randomization"
		}
		return nil
	}
	return nil
}

// AnimationDone must be called when the current animation finishes.
func (ds *DemoState) AnimationDone() {
	ds.animActive = false
}

func (ds *DemoState) nextShuffleAnim(p *Puzzle) *Animation {
	if ds.MoveIndex == 0 {
		ds.log().Info("start of randomization", "moves", len(ds.Moves))
	}
	if ds.MoveIndex >= len(ds.Moves) {
		ds.log().Info("done randomizing")
		ds.Phase = PhaseShufflePause
		ds.PauseRemaining = ShufflePauseDuration
		return nil
	}
	m := ds.Moves[ds.MoveIndex]
	ds.MoveIndex++
	ds.animActive = true
	return NewSliceAnimation(p, m.Axis, m.Slice, m.Turns, ShuffleTurnDuration)
}

func (ds *DemoState) nextSolveAnim(p *Puzzle) *Animation {
	if ds.MoveIndex >= len(ds.Moves) {
		ds.log().Info("done playing solution")
		ds.Phase = PhaseSolvePause
		ds.PauseRemaining = SolvePauseDuration
		return nil
	}
	m := ds.Moves[ds.MoveIndex]
	ds.MoveIndex++
	ds.animActive = true
	return NewSliceAnimation(p, m.Axis, m.Slice, m.Turns, SolveTurnDuration)
}

func generateShuffleMoves(p *Puzzle) []SliceMove {
	n := MinShuffleMoves + rand.IntN(MaxShuffleMoves-MinShuffleMoves+1)
	moves := make([]SliceMove, n)
	for i := range moves {
		axis := Axis(rand.IntN(3))
		slice := rand.IntN(p.Size[int(axis)])
		turns := 1
		if p.CanTurn90(axis) {
			// pick from {1, -1, 2}
			switch rand.IntN(3) {
			case 1:
				turns = -1
			case 2:
				turns = 2
			}
		} else {
			turns = 2 // only half turns allowed
		}
		moves[i] = SliceMove{Axis: axis, Slice: slice, Turns: turns}
	}
	return moves
}
