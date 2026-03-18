package puzzle

// Axis represents one of the three coordinate axes.
type Axis int

const (
	AxisX Axis = iota
	AxisY
	AxisZ
)

// Face represents one of the six cube faces.
type Face int

const (
	PosX Face = iota
	NegX
	PosY
	NegY
	PosZ
	NegZ
)

// Color represents a face color as a palette index.
type Color byte

const (
	Black  Color = 0
	White  Color = 1
	Red    Color = 2
	Orange Color = 3
	Blue   Color = 4
	Green  Color = 5
	Yellow Color = 6
)

// faceColor maps each face direction to its solved color.
var faceColor = [6]Color{
	PosX: Red,
	NegX: Orange,
	PosY: White,
	NegY: Yellow,
	PosZ: Blue,
	NegZ: Green,
}

// Cubie represents a single sub-block of the puzzle.
type Cubie struct {
	Pos    [3]int
	Colors [6]Color // indexed by Face
}

// Puzzle represents a rectangular prism block puzzle.
type Puzzle struct {
	Size   [3]int
	Cubies []Cubie
}

// NewPuzzle creates a solved puzzle of the given dimensions.
// Only cubies with at least one exposed face are stored.
func NewPuzzle(sx, sy, sz int) *Puzzle {
	p := &Puzzle{Size: [3]int{sx, sy, sz}}

	for x := range sx {
		for y := range sy {
			for z := range sz {
				// Skip fully interior cubies
				if x > 0 && x < sx-1 && y > 0 && y < sy-1 && z > 0 && z < sz-1 {
					continue
				}
				c := Cubie{Pos: [3]int{x, y, z}}
				if x == sx-1 {
					c.Colors[PosX] = faceColor[PosX]
				}
				if x == 0 {
					c.Colors[NegX] = faceColor[NegX]
				}
				if y == sy-1 {
					c.Colors[PosY] = faceColor[PosY]
				}
				if y == 0 {
					c.Colors[NegY] = faceColor[NegY]
				}
				if z == sz-1 {
					c.Colors[PosZ] = faceColor[PosZ]
				}
				if z == 0 {
					c.Colors[NegZ] = faceColor[NegZ]
				}
				p.Cubies = append(p.Cubies, c)
			}
		}
	}
	return p
}

// CanTurn90 returns true if the two non-axis dimensions are equal,
// allowing 90-degree turns.
func (p *Puzzle) CanTurn90(axis Axis) bool {
	switch axis {
	case AxisX:
		return p.Size[1] == p.Size[2]
	case AxisY:
		return p.Size[0] == p.Size[2]
	case AxisZ:
		return p.Size[0] == p.Size[1]
	}
	return false
}

// 90° positive face color cycles per axis.
// Each cycle lists faces that rotate into each other.
var cycles = [3][4]Face{
	AxisX: {PosY, PosZ, NegY, NegZ},
	AxisY: {PosX, PosZ, NegX, NegZ},
	AxisZ: {PosX, PosY, NegX, NegY},
}

// Turn rotates cubies with Pos[axis] > layer around the given axis.
// If CanTurn90, a 90° turn is performed; otherwise 180°.
// positive=true means counter-clockwise when looking from positive axis toward origin.
func (p *Puzzle) Turn(axis Axis, layer int, positive bool) {
	is90 := p.CanTurn90(axis)

	// Determine the two perpendicular axis indices
	var a1, a2 int
	switch axis {
	case AxisX:
		a1, a2 = 1, 2
	case AxisY:
		a1, a2 = 0, 2
	case AxisZ:
		a1, a2 = 0, 1
	}

	// Center of rotation in the perpendicular plane
	c1 := float64(p.Size[a1]-1) / 2.0
	c2 := float64(p.Size[a2]-1) / 2.0

	for i := range p.Cubies {
		if p.Cubies[i].Pos[int(axis)] <= layer {
			continue
		}

		// Rotate grid position
		v1 := float64(p.Cubies[i].Pos[a1]) - c1
		v2 := float64(p.Cubies[i].Pos[a2]) - c2

		if is90 {
			if positive {
				// 90° positive: (v1, v2) -> (-v2, v1)
				v1, v2 = -v2, v1
			} else {
				// 90° negative: (v1, v2) -> (v2, -v1)
				v1, v2 = v2, -v1
			}
		} else {
			// 180°: (v1, v2) -> (-v1, -v2)
			v1, v2 = -v1, -v2
		}

		p.Cubies[i].Pos[a1] = int(v1 + c1 + 0.5)
		p.Cubies[i].Pos[a2] = int(v2 + c2 + 0.5)

		// Cycle face colors
		cycle := cycles[axis]
		if is90 {
			if positive {
				// Forward cycle: 0->1->2->3->0
				tmp := p.Cubies[i].Colors[cycle[3]]
				p.Cubies[i].Colors[cycle[3]] = p.Cubies[i].Colors[cycle[2]]
				p.Cubies[i].Colors[cycle[2]] = p.Cubies[i].Colors[cycle[1]]
				p.Cubies[i].Colors[cycle[1]] = p.Cubies[i].Colors[cycle[0]]
				p.Cubies[i].Colors[cycle[0]] = tmp
			} else {
				// Reverse cycle: 3->2->1->0->3
				tmp := p.Cubies[i].Colors[cycle[0]]
				p.Cubies[i].Colors[cycle[0]] = p.Cubies[i].Colors[cycle[1]]
				p.Cubies[i].Colors[cycle[1]] = p.Cubies[i].Colors[cycle[2]]
				p.Cubies[i].Colors[cycle[2]] = p.Cubies[i].Colors[cycle[3]]
				p.Cubies[i].Colors[cycle[3]] = tmp
			}
		} else {
			// 180°: swap opposite pairs
			p.Cubies[i].Colors[cycle[0]], p.Cubies[i].Colors[cycle[2]] = p.Cubies[i].Colors[cycle[2]], p.Cubies[i].Colors[cycle[0]]
			p.Cubies[i].Colors[cycle[1]], p.Cubies[i].Colors[cycle[3]] = p.Cubies[i].Colors[cycle[3]], p.Cubies[i].Colors[cycle[1]]
		}
	}
}

// SliceTurn rotates a single slice at the given position around the axis.
// pos is 0-indexed: 0 is the first slice, Size[axis]-1 is the last.
func (p *Puzzle) SliceTurn(axis Axis, pos int, positive bool) {
	if pos == p.Size[int(axis)]-1 {
		// Outermost slice: Turn(axis, pos-1) affects only this slice.
		p.Turn(axis, pos-1, positive)
		return
	}
	// Turn everything from pos onward, then undo everything from pos+1 onward.
	p.Turn(axis, pos-1, positive)
	p.Turn(axis, pos, !positive)
}

// Clone returns a deep copy of the puzzle.
func (p *Puzzle) Clone() *Puzzle {
	clone := &Puzzle{Size: p.Size}
	clone.Cubies = make([]Cubie, len(p.Cubies))
	copy(clone.Cubies, p.Cubies)
	return clone
}
