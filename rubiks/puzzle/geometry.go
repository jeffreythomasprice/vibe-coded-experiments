package puzzle

import "experiment/graphics"

// GenerateTriangles converts the puzzle state into a list of triangles for rendering.
// gap controls the spacing between cubies (e.g. 0.05).
func GenerateTriangles(p *Puzzle, gap float64) []graphics.Triangle {
	// Center offset so puzzle is at origin
	cx := float64(p.Size[0]-1) / 2.0
	cy := float64(p.Size[1]-1) / 2.0
	cz := float64(p.Size[2]-1) / 2.0

	h := 0.5 - gap
	var tris []graphics.Triangle

	for _, c := range p.Cubies {
		ox := float64(c.Pos[0]) - cx
		oy := float64(c.Pos[1]) - cy
		oz := float64(c.Pos[2]) - cz

		tris = appendCubieFaces(tris, c, ox, oy, oz, h)
	}
	return tris
}

// GenerateTrianglesSplit generates triangles partitioned into stationary and
// rotating groups based on the turn parameters.
func GenerateTrianglesSplit(p *Puzzle, axis Axis, layer int) (stationary, rotating []graphics.Triangle) {
	cx := float64(p.Size[0]-1) / 2.0
	cy := float64(p.Size[1]-1) / 2.0
	cz := float64(p.Size[2]-1) / 2.0

	h := 0.5 - 0.05 // default gap

	for _, c := range p.Cubies {
		ox := float64(c.Pos[0]) - cx
		oy := float64(c.Pos[1]) - cy
		oz := float64(c.Pos[2]) - cz

		if c.Pos[int(axis)] > layer {
			rotating = appendCubieFaces(rotating, c, ox, oy, oz, h)
		} else {
			stationary = appendCubieFaces(stationary, c, ox, oy, oz, h)
		}
	}
	return
}

func appendCubieFaces(tris []graphics.Triangle, c Cubie, ox, oy, oz, h float64) []graphics.Triangle {
	type faceVerts struct {
		face    Face
		p0, p1 graphics.Vec3 // diagonal corners of the quad
		n1, n2 graphics.Vec3 // the two other corners
	}

	faces := [6]faceVerts{
		{PosX,
			graphics.Vec3{X: ox + h, Y: oy - h, Z: oz - h},
			graphics.Vec3{X: ox + h, Y: oy + h, Z: oz + h},
			graphics.Vec3{X: ox + h, Y: oy - h, Z: oz + h},
			graphics.Vec3{X: ox + h, Y: oy + h, Z: oz - h}},
		{NegX,
			graphics.Vec3{X: ox - h, Y: oy - h, Z: oz + h},
			graphics.Vec3{X: ox - h, Y: oy + h, Z: oz - h},
			graphics.Vec3{X: ox - h, Y: oy - h, Z: oz - h},
			graphics.Vec3{X: ox - h, Y: oy + h, Z: oz + h}},
		{PosY,
			graphics.Vec3{X: ox - h, Y: oy + h, Z: oz - h},
			graphics.Vec3{X: ox + h, Y: oy + h, Z: oz + h},
			graphics.Vec3{X: ox + h, Y: oy + h, Z: oz - h},
			graphics.Vec3{X: ox - h, Y: oy + h, Z: oz + h}},
		{NegY,
			graphics.Vec3{X: ox - h, Y: oy - h, Z: oz + h},
			graphics.Vec3{X: ox + h, Y: oy - h, Z: oz - h},
			graphics.Vec3{X: ox + h, Y: oy - h, Z: oz + h},
			graphics.Vec3{X: ox - h, Y: oy - h, Z: oz - h}},
		{PosZ,
			graphics.Vec3{X: ox - h, Y: oy - h, Z: oz + h},
			graphics.Vec3{X: ox + h, Y: oy + h, Z: oz + h},
			graphics.Vec3{X: ox - h, Y: oy + h, Z: oz + h},
			graphics.Vec3{X: ox + h, Y: oy - h, Z: oz + h}},
		{NegZ,
			graphics.Vec3{X: ox + h, Y: oy - h, Z: oz - h},
			graphics.Vec3{X: ox - h, Y: oy + h, Z: oz - h},
			graphics.Vec3{X: ox + h, Y: oy + h, Z: oz - h},
			graphics.Vec3{X: ox - h, Y: oy - h, Z: oz - h}},
	}

	for _, f := range faces {
		col := c.Colors[f.face]
		if col == Black {
			continue
		}
		tris = append(tris,
			graphics.Triangle{P0: f.p0, P1: f.p1, P2: f.n1, ColorIdx: byte(col)},
			graphics.Triangle{P0: f.p0, P1: f.n2, P2: f.p1, ColorIdx: byte(col)},
		)
	}
	return tris
}
