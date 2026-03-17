package graphics

import "math"

// Mat4 is a 4x4 matrix in column-major order: M[col*4+row].
type Mat4 [16]float64

// Identity returns the 4x4 identity matrix.
func Identity() Mat4 {
	var m Mat4
	m[0] = 1
	m[5] = 1
	m[10] = 1
	m[15] = 1
	return m
}

// Mul returns m * n (standard matrix multiplication).
func (m Mat4) Mul(n Mat4) Mat4 {
	var out Mat4
	for c := 0; c < 4; c++ {
		for r := 0; r < 4; r++ {
			var sum float64
			for k := 0; k < 4; k++ {
				sum += m[k*4+r] * n[c*4+k]
			}
			out[c*4+r] = sum
		}
	}
	return out
}

// TransformPoint transforms a point (w=1) through the matrix with perspective divide.
func (m Mat4) TransformPoint(v Vec3) Vec3 {
	p, _ := m.TransformPointW(v)
	return p
}

// TransformPointW transforms a point (w=1) and returns the divided result and raw w.
func (m Mat4) TransformPointW(v Vec3) (Vec3, float64) {
	x := m[0]*v.X + m[4]*v.Y + m[8]*v.Z + m[12]
	y := m[1]*v.X + m[5]*v.Y + m[9]*v.Z + m[13]
	z := m[2]*v.X + m[6]*v.Y + m[10]*v.Z + m[14]
	w := m[3]*v.X + m[7]*v.Y + m[11]*v.Z + m[15]
	if w == 0 {
		return Vec3{x, y, z}, w
	}
	return Vec3{x / w, y / w, z / w}, w
}

// Translate returns a translation matrix.
func Translate(dx, dy, dz float64) Mat4 {
	m := Identity()
	m[12] = dx
	m[13] = dy
	m[14] = dz
	return m
}

// RotateY returns a rotation matrix around the Y axis by angle radians.
func RotateY(angle float64) Mat4 {
	c, s := math.Cos(angle), math.Sin(angle)
	m := Identity()
	m[0] = c
	m[8] = s
	m[2] = -s
	m[10] = c
	return m
}

// RotateX returns a rotation matrix around the X axis by angle radians.
func RotateX(angle float64) Mat4 {
	c, s := math.Cos(angle), math.Sin(angle)
	m := Identity()
	m[5] = c
	m[9] = -s
	m[6] = s
	m[10] = c
	return m
}

// RotateZ returns a rotation matrix around the Z axis by angle radians.
func RotateZ(angle float64) Mat4 {
	c, s := math.Cos(angle), math.Sin(angle)
	m := Identity()
	m[0] = c
	m[4] = -s
	m[1] = s
	m[5] = c
	return m
}

// Perspective returns a perspective projection matrix.
// fovY is vertical field of view in radians.
func Perspective(fovY, aspect, near, far float64) Mat4 {
	f := 1.0 / math.Tan(fovY/2)
	var m Mat4
	m[0] = f / aspect
	m[5] = f
	m[10] = (far + near) / (near - far)
	m[11] = -1
	m[14] = (2 * far * near) / (near - far)
	return m
}

// LookAt returns a view matrix looking from eye toward center with the given up vector.
func LookAt(eye, center, up Vec3) Mat4 {
	f := center.Sub(eye).Normalize()
	s := f.Cross(up).Normalize()
	u := s.Cross(f)

	var m Mat4
	m[0] = s.X
	m[4] = s.Y
	m[8] = s.Z
	m[1] = u.X
	m[5] = u.Y
	m[9] = u.Z
	m[2] = -f.X
	m[6] = -f.Y
	m[10] = -f.Z
	m[12] = -s.Dot(eye)
	m[13] = -u.Dot(eye)
	m[14] = f.Dot(eye)
	m[15] = 1
	return m
}
