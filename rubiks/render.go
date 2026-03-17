package main

import (
	"io"

	"experiment/graphics"
)

// RenderFrame clears the framebuffer, rasterizes triangles, and encodes the
// result as sixel output. This is the same code path used by the interactive
// demo loop.
func (e *Encoder) RenderFrame(w io.Writer, fb *graphics.Framebuffer,
	triangles []graphics.Triangle, mvp graphics.Mat4, palette Palette) error {

	fb.Clear()
	for _, t := range triangles {
		fb.DrawTriangle(t.P0, t.P1, t.P2, t.ColorIdx, mvp)
	}
	return e.Encode(w, fb.Pixels, palette)
}
