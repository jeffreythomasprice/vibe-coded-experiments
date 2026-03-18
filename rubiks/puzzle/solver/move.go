package solver

import "experiment/puzzle"

// Move represents a single move on the puzzle.
type Move struct {
	Axis  puzzle.Axis
	Slice int // 0-indexed slice position
	Turns int // 1 = CW quarter, -1 = CCW quarter, 2 = half turn
}

// Apply performs the move on the puzzle.
func (m Move) Apply(p *puzzle.Puzzle) {
	switch m.Turns {
	case 1:
		p.SliceTurn(m.Axis, m.Slice, true)
	case -1:
		p.SliceTurn(m.Axis, m.Slice, false)
	case 2:
		if p.CanTurn90(m.Axis) {
			p.SliceTurn(m.Axis, m.Slice, true)
			p.SliceTurn(m.Axis, m.Slice, true)
		} else {
			// Turn already does 180° when CanTurn90 is false.
			p.SliceTurn(m.Axis, m.Slice, true)
		}
	}
}

// Unapply reverses the move on the puzzle.
func (m Move) Unapply(p *puzzle.Puzzle) {
	switch m.Turns {
	case 1:
		p.SliceTurn(m.Axis, m.Slice, false)
	case -1:
		p.SliceTurn(m.Axis, m.Slice, true)
	case 2:
		// Half turn is self-inverse.
		m.Apply(p)
	}
}

// Inverse returns the move that undoes this move.
func (m Move) Inverse() Move {
	inv := m
	switch m.Turns {
	case 1:
		inv.Turns = -1
	case -1:
		inv.Turns = 1
	}
	return inv
}

// GenerateMoves returns all legal moves for the puzzle.
func GenerateMoves(p *puzzle.Puzzle) []Move {
	var moves []Move
	for axis := puzzle.AxisX; axis <= puzzle.AxisZ; axis++ {
		for slice := range p.Size[int(axis)] {
			if p.CanTurn90(axis) {
				moves = append(moves, Move{Axis: axis, Slice: slice, Turns: 1})
				moves = append(moves, Move{Axis: axis, Slice: slice, Turns: -1})
				moves = append(moves, Move{Axis: axis, Slice: slice, Turns: 2})
			} else {
				moves = append(moves, Move{Axis: axis, Slice: slice, Turns: 2})
			}
		}
	}
	return moves
}
