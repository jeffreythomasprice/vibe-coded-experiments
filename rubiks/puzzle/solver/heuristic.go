package solver

import (
	"sort"

	"experiment/puzzle"
)

// colorSig identifies a cubie by its non-Black colors, sorted and zero-padded.
type colorSig [3]puzzle.Color

// cubieColorSig extracts the sorted non-Black colors from a cubie.
func cubieColorSig(c puzzle.Cubie) colorSig {
	var sig colorSig
	n := 0
	for f := puzzle.Face(0); f < 6; f++ {
		if c.Colors[f] != puzzle.Black {
			sig[n] = c.Colors[f]
			n++
		}
	}
	sort.Slice(sig[:n], func(i, j int) bool { return sig[i] < sig[j] })
	return sig
}

// cubieClass returns the number of non-Black faces (1=center, 2=edge, 3=corner).
func cubieClass(c puzzle.Cubie) int {
	n := 0
	for f := puzzle.Face(0); f < 6; f++ {
		if c.Colors[f] != puzzle.Black {
			n++
		}
	}
	return n
}

// axisDisplacement counts axes where pos and home differ.
func axisDisplacement(pos, home [3]int) int {
	d := 0
	for a := 0; a < 3; a++ {
		if pos[a] != home[a] {
			d++
		}
	}
	return d
}

// isOriented returns true if a cubie at its home position has the correct
// color on every face.
func isOriented(c puzzle.Cubie, p *puzzle.Puzzle) bool {
	for f := puzzle.Face(0); f < 6; f++ {
		if c.Colors[f] != SolvedColor(p, c.Pos, f) {
			return false
		}
	}
	return true
}

// heuristicState holds precomputed data for the Manhattan-distance heuristic.
type heuristicState struct {
	homes           map[colorSig][][3]int
	maxDispPerMove  int    // max total displacement any single move introduces
	maxDispPerClass [4]int // indexed by class (1=center, 2=edge, 3=corner)
}

// newHeuristicState builds the heuristic from a puzzle and its legal moves.
// The puzzle must be in its current (possibly scrambled) state, but we derive
// the solved state from the puzzle's dimensions.
func newHeuristicState(p *puzzle.Puzzle, moves []Move) *heuristicState {
	solved := puzzle.NewPuzzle(p.Size[0], p.Size[1], p.Size[2])

	// Build home position index from solved puzzle.
	homes := make(map[colorSig][][3]int)
	for _, c := range solved.Cubies {
		sig := cubieColorSig(c)
		homes[sig] = append(homes[sig], c.Pos)
	}

	// Measure max displacement introduced by each move.
	maxDisp := 0
	var maxDispClass [4]int
	for _, m := range moves {
		test := solved.Clone()
		m.Apply(test)
		d := totalDisplacement(test, homes)
		if d > maxDisp {
			maxDisp = d
		}
		for cls := 1; cls <= 3; cls++ {
			cd := classDisplacement(test, homes, cls)
			if cd > maxDispClass[cls] {
				maxDispClass[cls] = cd
			}
		}
	}

	// Ensure we never divide by zero.
	if maxDisp == 0 {
		maxDisp = 1
	}
	for i := 1; i <= 3; i++ {
		if maxDispClass[i] == 0 {
			maxDispClass[i] = 1
		}
	}

	return &heuristicState{
		homes:           homes,
		maxDispPerMove:  maxDisp,
		maxDispPerClass: maxDispClass,
	}
}

// Heuristic returns an admissible lower bound on the number of moves needed.
func (h *heuristicState) Heuristic(p *puzzle.Puzzle) int {
	// Total displacement heuristic.
	total := totalDisplacement(p, h.homes)
	best := ceilDiv(total, h.maxDispPerMove)

	// Per-class heuristics — max of admissible bounds is admissible.
	for cls := 1; cls <= 3; cls++ {
		cd := classDisplacement(p, h.homes, cls)
		if cd == 0 {
			continue
		}
		v := ceilDiv(cd, h.maxDispPerClass[cls])
		if v > best {
			best = v
		}
	}

	return best
}

// totalDisplacement sums per-cubie displacement (axis mismatch + misorientation).
func totalDisplacement(p *puzzle.Puzzle, homes map[colorSig][][3]int) int {
	sum := 0
	for _, c := range p.Cubies {
		sum += cubieDisplacement(c, p, homes)
	}
	return sum
}

// classDisplacement sums displacement for cubies of a given class.
func classDisplacement(p *puzzle.Puzzle, homes map[colorSig][][3]int, class int) int {
	sum := 0
	for _, c := range p.Cubies {
		if cubieClass(c) != class {
			continue
		}
		sum += cubieDisplacement(c, p, homes)
	}
	return sum
}

// cubieDisplacement returns the minimum displacement for a cubie across all
// valid home positions. Adds 1 for misoriented-in-place cubies.
func cubieDisplacement(c puzzle.Cubie, p *puzzle.Puzzle, homes map[colorSig][][3]int) int {
	sig := cubieColorSig(c)
	homeList := homes[sig]
	if len(homeList) == 0 {
		return 0
	}

	minD := 4 // max possible axis displacement is 3
	for _, home := range homeList {
		d := axisDisplacement(c.Pos, home)
		if d < minD {
			minD = d
		}
	}

	// If at home position but misoriented, it still needs at least one move.
	if minD == 0 && !isOriented(c, p) {
		minD = 1
	}

	return minD
}

func ceilDiv(a, b int) int {
	return (a + b - 1) / b
}
