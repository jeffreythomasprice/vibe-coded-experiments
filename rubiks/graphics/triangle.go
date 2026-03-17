package graphics

// Triangle defines a triangle by three vertices and a flat color index.
type Triangle struct {
	P0, P1, P2 Vec3
	ColorIdx   byte
}
