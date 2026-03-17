package main

import (
	"image"
	"image/color"
	"io"

	"experiment/graphics"
)

// RenderFrame clears the framebuffer, rasterizes triangles, and encodes the
// result as sixel output. Vertices are consumed in groups of 3 (one triangle
// each). This is the same code path used by the interactive demo loop.
func (e *Encoder) RenderFrame(w io.Writer, fb *graphics.Framebuffer,
	vertices []graphics.Vertex, mvp graphics.Mat4, opts *QuantizeOptions) error {

	fb.Clear()
	for i := 0; i+2 < len(vertices); i += 3 {
		fb.DrawTriangle(vertices[i], vertices[i+1], vertices[i+2], mvp)
	}
	return e.EncodeImage(w, fb.Image, opts)
}

// mandelbrot generates a Mandelbrot set image at the given dimensions.
func mandelbrot(width, height, maxIter int) *image.RGBA {
	img := image.NewRGBA(image.Rect(0, 0, width, height))

	xMin, xMax := -2.5, 1.0
	yMin, yMax := -1.0, 1.0

	for py := range height {
		for px := range width {
			x0 := xMin + (xMax-xMin)*float64(px)/float64(width)
			y0 := yMin + (yMax-yMin)*float64(py)/float64(height)

			var x, y float64
			iter := 0
			for x*x+y*y <= 4 && iter < maxIter {
				x, y = x*x-y*y+x0, 2*x*y+y0
				iter++
			}

			if iter == maxIter {
				img.Set(px, py, color.RGBA{A: 255})
			} else {
				t := float64(iter) / float64(maxIter)
				r := uint8(9 * (1 - t) * t * t * t * 255)
				g := uint8(15 * (1 - t) * (1 - t) * t * t * 255)
				b := uint8(8.5 * (1 - t) * (1 - t) * (1 - t) * t * 255)
				img.Set(px, py, color.RGBA{R: r, G: g, B: b, A: 255})
			}
		}
	}
	return img
}
