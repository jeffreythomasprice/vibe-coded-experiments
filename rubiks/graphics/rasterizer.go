package graphics

import (
	"image"
	"image/color"
	"math"
)

// Vertex holds a position and RGBA color (each component 0–1).
type Vertex struct {
	Pos   Vec3
	Color [4]float64
}

// Framebuffer holds an RGBA image and a depth buffer for software rendering.
type Framebuffer struct {
	Width, Height int
	Image         *image.RGBA
	Depth         []float64
}

// NewFramebuffer allocates a framebuffer with depth initialized to +infinity.
func NewFramebuffer(w, h int) *Framebuffer {
	fb := &Framebuffer{
		Width:  w,
		Height: h,
		Image:  image.NewRGBA(image.Rect(0, 0, w, h)),
		Depth:  make([]float64, w*h),
	}
	fb.clearDepth()
	return fb
}

// Clear resets the color image to black and the depth buffer to +infinity.
func (fb *Framebuffer) Clear() {
	for i := range fb.Image.Pix {
		fb.Image.Pix[i] = 0
	}
	fb.clearDepth()
}

func (fb *Framebuffer) clearDepth() {
	for i := range fb.Depth {
		fb.Depth[i] = math.MaxFloat64
	}
}

// DrawTriangle transforms three vertices through mvp and rasterizes the triangle
// with color interpolation and depth testing.
func (fb *Framebuffer) DrawTriangle(v0, v1, v2 Vertex, mvp Mat4) {
	// Transform to clip space
	ndc0, w0 := mvp.TransformPointW(v0.Pos)
	ndc1, w1 := mvp.TransformPointW(v1.Pos)
	ndc2, w2 := mvp.TransformPointW(v2.Pos)

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

			// Interpolate color
			r := b0*v0.Color[0] + b1*v1.Color[0] + b2*v2.Color[0]
			g := b0*v0.Color[1] + b1*v1.Color[1] + b2*v2.Color[1]
			b := b0*v0.Color[2] + b1*v1.Color[2] + b2*v2.Color[2]
			a := b0*v0.Color[3] + b1*v1.Color[3] + b2*v2.Color[3]

			fb.Image.SetRGBA(px, py, color.RGBA{
				R: clamp8(r),
				G: clamp8(g),
				B: clamp8(b),
				A: clamp8(a),
			})
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

func clamp8(v float64) uint8 {
	if v <= 0 {
		return 0
	}
	if v >= 1 {
		return 255
	}
	return uint8(v * 255)
}
