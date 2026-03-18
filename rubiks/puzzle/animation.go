package puzzle

import (
	"math"

	"experiment/graphics"
)

// Animation tracks the state of an in-progress turn animation.
type Animation struct {
	Axis      Axis
	Layer     int
	Positive  bool
	Target    float64 // radians: π/2 or π
	Elapsed   float64 // seconds
	Duration  float64 // seconds (e.g. 0.4)
	SliceMode bool    // when true, animate a single slice instead of a layer cut
	SlicePos  int     // which slice to rotate (only used when SliceMode=true)
	Turns     int     // 1 (CW), -1 (CCW), or 2 (half turn)
}

// Progress returns the animation progress clamped to [0, 1].
func (a *Animation) Progress() float64 {
	t := a.Elapsed / a.Duration
	if t < 0 {
		return 0
	}
	if t > 1 {
		return 1
	}
	return t
}

// CurrentAngle returns the current rotation angle using smoothstep easing.
func (a *Animation) CurrentAngle() float64 {
	t := a.Progress()
	// Smoothstep: 3t² - 2t³
	smooth := t * t * (3 - 2*t)
	angle := a.Target * smooth
	if !a.Positive {
		angle = -angle
	}
	return angle
}

// Done returns true if the animation has completed.
func (a *Animation) Done() bool {
	return a.Elapsed >= a.Duration
}

// RotateAxis returns a rotation matrix for the given axis and angle.
func RotateAxis(axis Axis, angle float64) graphics.Mat4 {
	switch axis {
	case AxisX:
		return graphics.RotateX(angle)
	case AxisY:
		// Standard RotateY has the opposite sign convention from RotateX/Z
		// due to the cross product direction in the XZ plane. Negate to
		// match Turn()'s rotation direction.
		return graphics.RotateY(-angle)
	case AxisZ:
		return graphics.RotateZ(angle)
	}
	return graphics.Identity()
}

// NewAnimation creates an animation for a turn on the given puzzle.
func NewAnimation(p *Puzzle, axis Axis, layer int, positive bool, duration float64) *Animation {
	target := math.Pi / 2
	if !p.CanTurn90(axis) {
		target = math.Pi
	}
	return &Animation{
		Axis:     axis,
		Layer:    layer,
		Positive: positive,
		Target:   target,
		Duration: duration,
	}
}

// NewSliceAnimation creates an animation for a single-slice turn.
// turns: 1 (CW), -1 (CCW), or 2 (half turn).
func NewSliceAnimation(p *Puzzle, axis Axis, slicePos, turns int, duration float64) *Animation {
	positive := turns > 0
	target := math.Pi / 2
	if !p.CanTurn90(axis) || abs(turns) == 2 {
		target = math.Pi
	}
	return &Animation{
		Axis:      axis,
		SliceMode: true,
		SlicePos:  slicePos,
		Turns:     turns,
		Positive:  positive,
		Target:    target,
		Duration:  duration,
	}
}

func abs(x int) int {
	if x < 0 {
		return -x
	}
	return x
}

// ApplyTurn commits the animated turn to the puzzle state.
func (a *Animation) ApplyTurn(p *Puzzle) {
	if !a.SliceMode {
		p.Turn(a.Axis, a.Layer, a.Positive)
		return
	}
	switch a.Turns {
	case 1:
		p.SliceTurn(a.Axis, a.SlicePos, true)
	case -1:
		p.SliceTurn(a.Axis, a.SlicePos, false)
	case 2:
		if p.CanTurn90(a.Axis) {
			p.SliceTurn(a.Axis, a.SlicePos, true)
			p.SliceTurn(a.Axis, a.SlicePos, true)
		} else {
			p.SliceTurn(a.Axis, a.SlicePos, true)
		}
	}
}

// SplitTriangles partitions puzzle triangles into stationary and rotating groups.
func (a *Animation) SplitTriangles(p *Puzzle) (stationary, rotating []graphics.Triangle) {
	if a.SliceMode {
		return GenerateTrianglesSplitSlice(p, a.Axis, a.SlicePos)
	}
	return GenerateTrianglesSplit(p, a.Axis, a.Layer)
}
