package puzzle

import (
	"fmt"
	"math"
	"testing"

	"experiment/graphics"
)

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

func TestFourTurnsIdentityAllRotations(t *testing.T) {
	axes := []struct {
		name string
		axis Axis
	}{
		{"X", AxisX},
		{"Y", AxisY},
		{"Z", AxisZ},
	}

	// For a 3x3x3, Pos values are 0,1,2.
	// Turn rotates cubies with Pos[axis] > layer.
	// layer -1: all cubies; layer 0: pos 1,2; layer 1: pos 2 only.
	layers := []int{-1, 0, 1}

	for _, ax := range axes {
		for _, layer := range layers {
			for _, positive := range []bool{true, false} {
				dir := "positive"
				if !positive {
					dir = "negative"
				}
				name := fmt.Sprintf("axis=%s/layer=%d/%s", ax.name, layer, dir)
				t.Run(name, func(t *testing.T) {
					p := NewPuzzle(3, 3, 3)
					original := make([]Cubie, len(p.Cubies))
					copy(original, p.Cubies)

					for range 4 {
						p.Turn(ax.axis, layer, positive)
					}

					if !cubiesEqual(original, p.Cubies) {
						// Find which cubies differ for debugging
						for _, co := range original {
							found := false
							for _, cn := range p.Cubies {
								if co.Pos == cn.Pos && co.Colors == cn.Colors {
									found = true
									break
								}
							}
							if !found {
								// Find what's at this position now
								for _, cn := range p.Cubies {
									if co.Pos == cn.Pos {
										t.Errorf("pos %v: want colors %v, got %v", co.Pos, co.Colors, cn.Colors)
										break
									}
								}
							}
						}
						t.Errorf("4x 90° turn did not return to solved state")
					}
				})
			}
		}
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

func TestTurnColorDestination(t *testing.T) {
	// On a 3x3x3, do a positive Y turn of the top layer (layer=1).
	// The center cubie on +X face at y=2 starts at (2,2,1) with PosX=Red.
	// After a positive Y rotation, position (2,2,1) → (1,2,2).
	// The +X face (Red) should rotate onto the +Z face.
	p := NewPuzzle(3, 3, 3)
	p.Turn(AxisY, 1, true)

	var found bool
	for _, c := range p.Cubies {
		if c.Pos == [3]int{1, 2, 2} {
			found = true
			if c.Colors[PosZ] != Red {
				t.Errorf("after +Y turn, cubie at (1,2,2) PosZ = %d, want Red (%d)", c.Colors[PosZ], Red)
			}
			break
		}
	}
	if !found {
		t.Fatal("cubie at (1,2,2) not found after turn")
	}
}

// TestAnimationMatchesTurn verifies that the visual end-state of an animation
// (rotation matrix applied to face centers) matches the post-Turn() puzzle state.
// For each exposed face of each rotating cubie, we compute where the animation
// places that face's center, then check that the post-Turn() puzzle has the same
// color at that location. A mismatch means the animation rotates visually in one
// direction but Turn() moves colors in another.
func TestAnimationMatchesTurn(t *testing.T) {
	axes := []struct {
		name string
		axis Axis
	}{
		{"X", AxisX},
		{"Y", AxisY},
		{"Z", AxisZ},
	}
	layers := []int{-1, 0, 1}

	// Face normal offsets: face center = cubie center + offset * 0.5
	faceOffset := [6]graphics.Vec3{
		PosX: {X: 1},
		NegX: {X: -1},
		PosY: {Y: 1},
		NegY: {Y: -1},
		PosZ: {Z: 1},
		NegZ: {Z: -1},
	}

	for _, ax := range axes {
		for _, layer := range layers {
			for _, positive := range []bool{true, false} {
				dir := "positive"
				if !positive {
					dir = "negative"
				}
				name := fmt.Sprintf("axis=%s/layer=%d/%s", ax.name, layer, dir)
				t.Run(name, func(t *testing.T) {
					p := NewPuzzle(3, 3, 3)
					cx := float64(p.Size[0]-1) / 2.0
					cy := float64(p.Size[1]-1) / 2.0
					cz := float64(p.Size[2]-1) / 2.0

					// Build rotation matrix for the completed animation
					angle := math.Pi / 2
					if !positive {
						angle = -angle
					}
					rotMat := RotateAxis(ax.axis, angle)

					// Collect animated face destinations for rotating cubies.
					// We separately rotate the cubie center (to find new grid pos)
					// and the face normal (to find which face it maps to).
					type faceSample struct {
						newPos  [3]int
						newFace Face
						color   Color
					}
					var samples []faceSample

					for _, c := range p.Cubies {
						if c.Pos[int(ax.axis)] <= layer {
							continue
						}
						cubieCenter := graphics.Vec3{
							X: float64(c.Pos[0]) - cx,
							Y: float64(c.Pos[1]) - cy,
							Z: float64(c.Pos[2]) - cz,
						}
						// Rotate the cubie center to find new grid position
						rc := rotMat.TransformPoint(cubieCenter)
						newPos := [3]int{
							int(math.Round(rc.X + cx)),
							int(math.Round(rc.Y + cy)),
							int(math.Round(rc.Z + cz)),
						}

						for f := Face(0); f < 6; f++ {
							if c.Colors[f] == Black {
								continue
							}
							// Rotate the face normal to find which face it becomes
							rn := rotMat.TransformPoint(faceOffset[f])
							var newFace Face
							switch {
							case rn.X > 0.5:
								newFace = PosX
							case rn.X < -0.5:
								newFace = NegX
							case rn.Y > 0.5:
								newFace = PosY
							case rn.Y < -0.5:
								newFace = NegY
							case rn.Z > 0.5:
								newFace = PosZ
							case rn.Z < -0.5:
								newFace = NegZ
							}
							samples = append(samples, faceSample{
								newPos:  newPos,
								newFace: newFace,
								color:   c.Colors[f],
							})
						}
					}

					// Now apply the logical turn
					p.Turn(ax.axis, layer, positive)

					// For each sample, verify the post-Turn() puzzle agrees.
					for _, s := range samples {
						var found bool
						for _, c := range p.Cubies {
							if c.Pos == s.newPos {
								found = true
								if c.Colors[s.newFace] != s.color {
									t.Errorf("pos %v face %d: animation expects color %d, Turn() has color %d",
										s.newPos, s.newFace, s.color, c.Colors[s.newFace])
								}
								break
							}
						}
						if !found {
							t.Errorf("no cubie at %v after Turn()", s.newPos)
						}
					}
				})
			}
		}
	}
}

func TestSliceTurnOuterIdentity(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	original := p.Clone()

	for range 4 {
		p.SliceTurn(AxisY, 2, true)
	}
	if !cubiesEqual(original.Cubies, p.Cubies) {
		t.Error("4x SliceTurn on outer slice did not return to identity")
	}
}

func TestSliceTurnMiddleIdentity(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	original := p.Clone()

	for range 4 {
		p.SliceTurn(AxisY, 1, true)
	}
	if !cubiesEqual(original.Cubies, p.Cubies) {
		t.Error("4x SliceTurn on middle slice did not return to identity")
	}
}

func TestSliceTurnSlice0Identity(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	original := p.Clone()

	for range 4 {
		p.SliceTurn(AxisX, 0, true)
	}
	if !cubiesEqual(original.Cubies, p.Cubies) {
		t.Error("4x SliceTurn on slice 0 did not return to identity")
	}
}

func TestSliceTurnOnlyAffectsTargetSlice(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	original := p.Clone()

	p.SliceTurn(AxisY, 1, true)

	for _, c := range original.Cubies {
		if c.Pos[1] == 1 {
			continue
		}
		found := false
		for _, c2 := range p.Cubies {
			if c.Pos == c2.Pos && c.Colors == c2.Colors {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("cubie at %v was changed by middle slice turn", c.Pos)
		}
	}
}

func TestSliceTurn180Identity(t *testing.T) {
	// 2x3x4: AxisY does 180° turns. Two 180° turns = identity.
	p := NewPuzzle(2, 3, 4)
	original := p.Clone()

	p.SliceTurn(AxisY, 1, true)
	p.SliceTurn(AxisY, 1, true)

	if !cubiesEqual(original.Cubies, p.Cubies) {
		t.Error("2x SliceTurn 180° did not return to identity")
	}
}

func TestClone(t *testing.T) {
	p := NewPuzzle(3, 3, 3)
	clone := p.Clone()

	p.Turn(AxisY, 0, true)

	original := NewPuzzle(3, 3, 3)
	if !cubiesEqual(clone.Cubies, original.Cubies) {
		t.Error("clone was modified when original was turned")
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
