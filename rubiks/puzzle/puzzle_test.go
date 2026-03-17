package puzzle

import "testing"

func TestCubieCount2x2x2(t *testing.T) {
	p := NewPuzzle(2, 2, 2)
	if len(p.Cubies) != 8 {
		t.Errorf("2x2x2 cubie count = %d, want 8", len(p.Cubies))
	}
}

func TestCubieCount3x3x3(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	if len(p.Cubies) != 26 {
		t.Errorf("3x3x3 cubie count = %d, want 26", len(p.Cubies))
	}
}

func TestSolvedCornerHas3Colors(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	// Corner at (0,0,0) should have 3 non-Black faces: NegX, NegY, NegZ
	var corner *Cubie
	for i := range p.Cubies {
		if p.Cubies[i].Pos == [3]int{0, 0, 0} {
			corner = &p.Cubies[i]
			break
		}
	}
	if corner == nil {
		t.Fatal("corner cubie (0,0,0) not found")
	}
	count := 0
	for _, c := range corner.Colors {
		if c != Black {
			count++
		}
	}
	if count != 3 {
		t.Errorf("corner has %d non-black faces, want 3", count)
	}
}

func TestSolvedEdgeHas2Colors(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	// Edge at (1,0,0) should have 2 non-Black faces: NegY, NegZ
	var edge *Cubie
	for i := range p.Cubies {
		if p.Cubies[i].Pos == [3]int{1, 0, 0} {
			edge = &p.Cubies[i]
			break
		}
	}
	if edge == nil {
		t.Fatal("edge cubie (1,0,0) not found")
	}
	count := 0
	for _, c := range edge.Colors {
		if c != Black {
			count++
		}
	}
	if count != 2 {
		t.Errorf("edge has %d non-black faces, want 2", count)
	}
}

func TestSolvedCenterHas1Color(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	// Center at (1,1,0) should have 1 non-Black face: NegZ
	var center *Cubie
	for i := range p.Cubies {
		if p.Cubies[i].Pos == [3]int{1, 1, 0} {
			center = &p.Cubies[i]
			break
		}
	}
	if center == nil {
		t.Fatal("center cubie (1,1,0) not found")
	}
	count := 0
	for _, c := range center.Colors {
		if c != Black {
			count++
		}
	}
	if count != 1 {
		t.Errorf("center has %d non-black faces, want 1", count)
	}
}

func TestFourTurnsIdentity(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	original := make([]Cubie, len(p.Cubies))
	copy(original, p.Cubies)

	for range 4 {
		p.Turn(AxisY, 0, true)
	}

	if !cubiesEqual(original, p.Cubies) {
		t.Error("4x same 90° turn did not return to identity")
	}
}

func TestTwoTurns180Identity(t *testing.T) {
	// 2x3x4 cannot do 90° around Y (sizes 2 and 4 differ), so it does 180°
	p := NewPuzzle(2, 3, 4)
	original := make([]Cubie, len(p.Cubies))
	copy(original, p.Cubies)

	p.Turn(AxisY, 0, true)
	p.Turn(AxisY, 0, true)

	if !cubiesEqual(original, p.Cubies) {
		t.Error("2x 180° turn did not return to identity")
	}
}

func TestCanTurn90(t *testing.T) {
	tests := []struct {
		sx, sy, sz int
		axis       Axis
		want       bool
	}{
		{3, 3, 3, AxisX, true},
		{3, 3, 3, AxisY, true},
		{3, 3, 3, AxisZ, true},
		{2, 3, 4, AxisX, false}, // Y≠Z
		{2, 3, 4, AxisY, false}, // X≠Z
		{2, 3, 4, AxisZ, false}, // X≠Y
		{3, 2, 2, AxisX, true},  // Y==Z
		{3, 2, 2, AxisY, false}, // X≠Z
	}
	for _, tt := range tests {
		p := NewPuzzle(tt.sx, tt.sy, tt.sz)
		got := p.CanTurn90(tt.axis)
		if got != tt.want {
			t.Errorf("CanTurn90(%dx%dx%d, %d) = %v, want %v",
				tt.sx, tt.sy, tt.sz, tt.axis, got, tt.want)
		}
	}
}

func TestTurnChangesColors(t *testing.T) {
	p := NewPuzzle(3, 3, 3)

	// Find the center cubie on the +Z face at the top (y=2) before turn
	var before Color
	for _, c := range p.Cubies {
		if c.Pos == [3]int{1, 2, 2} {
			before = c.Colors[PosZ]
			break
		}
	}

	// Turn top layer around Y (layer=1 rotates cubies with y>1, i.e. y=2)
	p.Turn(AxisY, 1, true)

	// After rotating the top layer, position (1,2,2) should have a different cubie
	var after Color
	for _, c := range p.Cubies {
		if c.Pos == [3]int{1, 2, 2} {
			after = c.Colors[PosZ]
			break
		}
	}

	if before == after {
		t.Error("turn did not change the +Z face color at top edge")
	}
}

// cubiesEqual compares two cubie slices ignoring order.
func cubiesEqual(a, b []Cubie) bool {
	if len(a) != len(b) {
		return false
	}
	for _, ca := range a {
		found := false
		for _, cb := range b {
			if ca.Pos == cb.Pos && ca.Colors == cb.Colors {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}
	return true
}
