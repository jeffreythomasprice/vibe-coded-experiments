package main

import "io"

// Encode writes sixel-encoded output to w from indexed pixel data.
//
// pixels must have length width*height, where each byte is a palette index
// (0 to palette.Size-1). The pixel at (x, y) is pixels[y*width + x].
//
// This uses the column-first scatter algorithm (Option B): for each band,
// each pixel is read exactly once and its bit is scattered into the
// per-color band buffer. Then each active color's buffer is RLE-encoded.
func (e *Encoder) Encode(w io.Writer, pixels []byte, palette Palette) error {
	width := e.width
	height := e.height

	if len(pixels) != width*height {
		return ErrPixelLength{Expected: width * height, Got: len(pixels)}
	}

	out := e.outBuf[:0]

	// DCS q — enter sixel mode.
	// Set raster attributes: Pan=0;Pad=0;Ph=width;Pv=height
	out = append(out, DCS...)
	out = append(out, '"')
	out = appendDecimal(out, 1) // Pan (aspect ratio numerator)
	out = append(out, ';')
	out = appendDecimal(out, 1) // Pad (aspect ratio denominator)
	out = append(out, ';')
	out = appendDecimal(out, width)
	out = append(out, ';')
	out = appendDecimal(out, height)

	// Emit palette definitions.
	for i := range palette.Size {
		c := palette.Colors[i]
		// #N;2;R;G;B  (RGB percentages 0–100)
		out = append(out, '#')
		out = appendDecimal(out, i)
		out = append(out, ";2;"...)
		out = appendDecimal(out, int(c[0])*100/255)
		out = append(out, ';')
		out = appendDecimal(out, int(c[1])*100/255)
		out = append(out, ';')
		out = appendDecimal(out, int(c[2])*100/255)
	}

	// Process each 6-row band.
	bands := (height + BandHeight - 1) / BandHeight
	for band := range bands {
		bandY := band * BandHeight
		rowsInBand := BandHeight
		if bandY+rowsInBand > height {
			rowsInBand = height - bandY
		}

		// Clear scatter buffers and active list.
		e.active = e.active[:0]
		for i := range e.colorSeen {
			e.colorSeen[i] = false
		}

		// Scatter pass: walk each column once, read 6 pixels, scatter bits.
		for col := range width {
			for row := range rowsInBand {
				idx := pixels[(bandY+row)*width+col]
				if idx >= byte(palette.Size) {
					continue
				}
				e.scatter[idx][col] |= 1 << uint(row)
				if !e.colorSeen[idx] {
					e.colorSeen[idx] = true
					e.active = append(e.active, int(idx))
				}
			}
		}

		// Encode each active color.
		for ci, colorIdx := range e.active {
			// Color introducer: #N
			out = append(out, '#')
			out = appendDecimal(out, colorIdx)

			// RLE-encode this color's scatter row.
			out = appendRLE(out, e.scatter[colorIdx][:width])

			// '$' = carriage return (same band, next color).
			// '-' = newline (next band) after last color.
			if ci < len(e.active)-1 {
				out = append(out, '$')
			}

			// Clear the scatter buffer for this color.
			clear(e.scatter[colorIdx][:width])
		}

		// Band separator: '-' moves to next band (except after last band).
		if band < bands-1 {
			out = append(out, '-')
		}
	}

	// ST — leave sixel mode.
	out = append(out, ST...)

	e.outBuf = out // keep the grown buffer for reuse
	_, err := w.Write(out)
	return err
}

// ErrPixelLength is returned when the pixel slice length doesn't match width*height.
type ErrPixelLength struct {
	Expected, Got int
}

func (e ErrPixelLength) Error() string {
	buf := make([]byte, 0, 64)
	buf = append(buf, "sixel: pixel slice length "...)
	buf = appendDecimal(buf, e.Got)
	buf = append(buf, " != expected "...)
	buf = appendDecimal(buf, e.Expected)
	return string(buf)
}
