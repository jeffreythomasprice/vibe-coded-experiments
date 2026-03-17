package puzzle

import (
	"math"

	"experiment/graphics"
)

// Animation tracks the state of an in-progress turn animation.
type Animation struct {
	Axis     Axis
	Layer    int
	Positive bool
	Target   float64 // radians: π/2 or π
	Elapsed  float64 // seconds
	Duration float64 // seconds (e.g. 0.4)
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
		return graphics.RotateY(angle)
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
