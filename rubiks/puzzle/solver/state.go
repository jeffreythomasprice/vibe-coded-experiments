package solver

import "experiment/puzzle"

// SolvedColor returns the expected color for a facelet at the given position
// and face in a solved puzzle.
func SolvedColor(p *puzzle.Puzzle, pos [3]int, f puzzle.Face) puzzle.Color {
	switch f {
	case puzzle.PosX:
		if pos[0] == p.Size[0]-1 {
			return puzzle.Red
		}
	case puzzle.NegX:
		if pos[0] == 0 {
			return puzzle.Orange
		}
	case puzzle.PosY:
		if pos[1] == p.Size[1]-1 {
			return puzzle.White
		}
	case puzzle.NegY:
		if pos[1] == 0 {
			return puzzle.Yellow
		}
	case puzzle.PosZ:
		if pos[2] == p.Size[2]-1 {
			return puzzle.Blue
		}
	case puzzle.NegZ:
		if pos[2] == 0 {
			return puzzle.Green
		}
	}
	return puzzle.Black
}

// IsSolved returns true if all facelets match their solved colors.
func IsSolved(p *puzzle.Puzzle) bool {
	for _, c := range p.Cubies {
		for f := puzzle.Face(0); f < 6; f++ {
			if c.Colors[f] != SolvedColor(p, c.Pos, f) {
				return false
			}
		}
	}
	return true
}

// CountMisplaced returns the number of facelets not matching their solved color.
func CountMisplaced(p *puzzle.Puzzle) int {
	count := 0
	for _, c := range p.Cubies {
		for f := puzzle.Face(0); f < 6; f++ {
			if c.Colors[f] != SolvedColor(p, c.Pos, f) {
				count++
			}
		}
	}
	return count
}
