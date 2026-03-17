package graphics

import "math"

// Framebuffer holds an indexed pixel buffer and a depth buffer for software rendering.
type Framebuffer struct {
	Width, Height int
	Pixels        []byte    // indexed color buffer, each byte is a palette index
	Depth         []float64 // depth buffer for z-testing
}

// NewFramebuffer allocates a framebuffer with depth initialized to +infinity.
func NewFramebuffer(w, h int) *Framebuffer {
	fb := &Framebuffer{
		Width:  w,
		Height: h,
		Pixels: make([]byte, w*h),
		Depth:  make([]float64, w*h),
	}
	fb.clearDepth()
	return fb
}

// Clear resets the pixel buffer to zero and the depth buffer to +infinity.
func (fb *Framebuffer) Clear() {
	clear(fb.Pixels)
	fb.clearDepth()
}

func (fb *Framebuffer) clearDepth() {
	for i := range fb.Depth {
		fb.Depth[i] = math.MaxFloat64
	}
}

// DrawTriangle transforms three points through mvp and rasterizes the triangle
// with a flat color index and depth testing.
func (fb *Framebuffer) DrawTriangle(p0, p1, p2 Vec3, colorIdx byte, mvp Mat4) {
	// Transform to clip space
	ndc0, w0 := mvp.TransformPointW(p0)
	ndc1, w1 := mvp.TransformPointW(p1)
	ndc2, w2 := mvp.TransformPointW(p2)

	// Discard if any vertex is behind the near plane
	if w0 <= 0 || w1 <= 0 || w2 <= 0 {
		return
	}

	// Viewport transform: NDC [-1,1] → screen pixels, Y flipped
	fw, fh := float64(fb.Width), float64(fb.Height)
	sx0, sy0 := (ndc0.X+1)*0.5*fw, (1-ndc0.Y)*0.5*fh
	sx1, sy1 := (ndc1.X+1)*0.5*fw, (1-ndc1.Y)*0.5*fh
	sx2, sy2 := (ndc2.X+1)*0.5*fw, (1-ndc2.Y)*0.5*fh

	// Bounding box clamped to framebuffer
	minX := int(math.Floor(min3(sx0, sx1, sx2)))
	maxX := int(math.Ceil(max3(sx0, sx1, sx2)))
	minY := int(math.Floor(min3(sy0, sy1, sy2)))
	maxY := int(math.Ceil(max3(sy0, sy1, sy2)))
	if minX < 0 {
		minX = 0
	}
	if minY < 0 {
		minY = 0
	}
	if maxX > fb.Width {
		maxX = fb.Width
	}
	if maxY > fb.Height {
		maxY = fb.Height
	}

	// Triangle area (2x) via cross product of edge vectors
	area := edgeFn(sx0, sy0, sx1, sy1, sx2, sy2)
	if area == 0 {
		return // degenerate
	}
	invArea := 1.0 / area

	for py := minY; py < maxY; py++ {
		for px := minX; px < maxX; px++ {
			// Sample at pixel center
			pcx, pcy := float64(px)+0.5, float64(py)+0.5

			// Barycentric coordinates
			b0 := edgeFn(sx1, sy1, sx2, sy2, pcx, pcy) * invArea
			b1 := edgeFn(sx2, sy2, sx0, sy0, pcx, pcy) * invArea
			b2 := edgeFn(sx0, sy0, sx1, sy1, pcx, pcy) * invArea

			// Accept both winding orders
			inside := (b0 >= 0 && b1 >= 0 && b2 >= 0) || (b0 <= 0 && b1 <= 0 && b2 <= 0)
			if !inside {
				continue
			}

			// Make barycentrics positive for interpolation
			if b0 < 0 {
				b0, b1, b2 = -b0, -b1, -b2
			}

			// Interpolate depth
			z := b0*ndc0.Z + b1*ndc1.Z + b2*ndc2.Z

			idx := py*fb.Width + px
			if z >= fb.Depth[idx] {
				continue
			}
			fb.Depth[idx] = z
			fb.Pixels[idx] = colorIdx
		}
	}
}

// edgeFn returns the signed area of the parallelogram formed by (ax,ay)→(bx,by)→(cx,cy).
func edgeFn(ax, ay, bx, by, cx, cy float64) float64 {
	return (bx-ax)*(cy-ay) - (by-ay)*(cx-ax)
}

func min3(a, b, c float64) float64 {
	return math.Min(a, math.Min(b, c))
}

func max3(a, b, c float64) float64 {
	return math.Max(a, math.Max(b, c))
}
