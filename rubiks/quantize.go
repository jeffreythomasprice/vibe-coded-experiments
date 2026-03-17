package main

import (
	"image"
	"image/color"
	"io"
	"sort"
)

// QuantizeOptions controls the quantization step of EncodeImage.
type QuantizeOptions struct {
	MaxColors int // 1–256, default 256
}

// EncodeImage quantizes an image to a palette and encodes it as sixel.
// This is a convenience wrapper around median-cut quantization + Encode.
func (e *Encoder) EncodeImage(w io.Writer, img image.Image, opts *QuantizeOptions) error {
	maxColors := MaxColors
	if opts != nil && opts.MaxColors > 0 && opts.MaxColors <= MaxColors {
		maxColors = opts.MaxColors
	}

	bounds := img.Bounds()
	width := bounds.Dx()
	height := bounds.Dy()

	// Resize encoder if dimensions changed.
	if width != e.width || height != e.height {
		*e = *NewEncoder(width, height)
	}

	// Collect unique RGB values (or sample for large images).
	rgbPixels := make([][3]byte, width*height)
	for y := range height {
		for x := range width {
			r, g, b, _ := img.At(bounds.Min.X+x, bounds.Min.Y+y).RGBA()
			rgbPixels[y*width+x] = [3]byte{byte(r >> 8), byte(g >> 8), byte(b >> 8)}
		}
	}

	palette := medianCut(rgbPixels, maxColors)

	// Map each pixel to nearest palette index.
	indexed := make([]byte, len(rgbPixels))
	for i, rgb := range rgbPixels {
		indexed[i] = nearestColor(rgb, palette)
	}

	return e.Encode(w, indexed, palette)
}

// nearestColor returns the palette index closest to rgb (Euclidean distance).
func nearestColor(rgb [3]byte, pal Palette) byte {
	best := 0
	bestDist := 1<<31 - 1
	for i := range pal.Size {
		c := pal.Colors[i]
		dr := int(rgb[0]) - int(c[0])
		dg := int(rgb[1]) - int(c[1])
		db := int(rgb[2]) - int(c[2])
		d := dr*dr + dg*dg + db*db
		if d < bestDist {
			bestDist = d
			best = i
			if d == 0 {
				break
			}
		}
	}
	return byte(best)
}

// medianCut performs median-cut color quantization, returning a palette
// with at most maxColors entries.
func medianCut(pixels [][3]byte, maxColors int) Palette {
	if len(pixels) == 0 {
		return Palette{Size: 1}
	}

	type box struct {
		pixels [][3]byte
	}

	// Work on a copy so we don't reorder the caller's slice.
	tmp := make([][3]byte, len(pixels))
	copy(tmp, pixels)

	boxes := []box{{pixels: tmp}}

	for len(boxes) < maxColors {
		// Find the box with the largest range on any axis.
		bestIdx := 0
		bestRange := 0
		bestAxis := 0
		for i, b := range boxes {
			if len(b.pixels) < 2 {
				continue
			}
			for axis := range 3 {
				lo, hi := b.pixels[0][axis], b.pixels[0][axis]
				for _, p := range b.pixels[1:] {
					if p[axis] < lo {
						lo = p[axis]
					}
					if p[axis] > hi {
						hi = p[axis]
					}
				}
				r := int(hi) - int(lo)
				if r > bestRange {
					bestRange = r
					bestIdx = i
					bestAxis = axis
				}
			}
		}

		if bestRange == 0 {
			break // all remaining boxes are single-color
		}

		// Sort the selected box along the best axis and split at median.
		b := boxes[bestIdx]
		axis := bestAxis
		sort.Slice(b.pixels, func(i, j int) bool {
			return b.pixels[i][axis] < b.pixels[j][axis]
		})
		mid := len(b.pixels) / 2
		boxes[bestIdx] = box{pixels: b.pixels[:mid]}
		boxes = append(boxes, box{pixels: b.pixels[mid:]})
	}

	// Average each box to get the palette color.
	var pal Palette
	pal.Size = len(boxes)
	for i, b := range boxes {
		var rSum, gSum, bSum int64
		for _, p := range b.pixels {
			rSum += int64(p[0])
			gSum += int64(p[1])
			bSum += int64(p[2])
		}
		n := int64(len(b.pixels))
		pal.Colors[i] = [3]byte{
			byte(rSum / n),
			byte(gSum / n),
			byte(bSum / n),
		}
	}
	return pal
}

// ImageToPaletted converts an image.Image to indexed pixels and a Palette
// using median-cut quantization. Useful when you want to quantize once and
// encode multiple times.
func ImageToPaletted(img image.Image, maxColors int) (pixels []byte, palette Palette) {
	if maxColors <= 0 || maxColors > MaxColors {
		maxColors = MaxColors
	}

	bounds := img.Bounds()
	width := bounds.Dx()
	height := bounds.Dy()

	rgbPixels := make([][3]byte, width*height)
	for y := range height {
		for x := range width {
			r, g, b, _ := img.At(bounds.Min.X+x, bounds.Min.Y+y).RGBA()
			rgbPixels[y*width+x] = [3]byte{byte(r >> 8), byte(g >> 8), byte(b >> 8)}
		}
	}

	palette = medianCut(rgbPixels, maxColors)
	pixels = make([]byte, len(rgbPixels))
	for i, rgb := range rgbPixels {
		pixels[i] = nearestColor(rgb, palette)
	}
	return pixels, palette
}

// Ensure color.Color interface is available for potential future use.
var _ color.Color = color.RGBA{}
