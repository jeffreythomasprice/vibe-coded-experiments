package main

// DCS introduces a sixel data stream: DCS q
const DCS = "\x1bPq"

// ST terminates a sixel data stream.
const ST = "\x1b\\"

// BandHeight is the number of pixel rows per sixel band.
const BandHeight = 6

// MaxColors is the maximum number of palette entries.
const MaxColors = 256

// Palette holds up to 256 RGB colors.
type Palette struct {
	Colors [MaxColors][3]byte // R, G, B (0–255)
	Size   int                // active colors (1–256)
}

// Encoder encodes indexed pixel data into sixel format.
// It pre-allocates internal buffers and is reusable across frames.
// Not safe for concurrent use; create one Encoder per goroutine.
type Encoder struct {
	width  int
	height int

	// scatter[color][col] holds the sixel bitmask for that color/column
	// within the current band. Allocated once, reused per band.
	scatter [MaxColors][]byte

	// active tracks which color indices appeared in the current band.
	active []int

	// colorSeen tracks whether a color has been seen in the current band.
	colorSeen [MaxColors]bool

	// outBuf is a reusable byte buffer for building the output.
	outBuf []byte
}

// NewEncoder creates an Encoder pre-allocated for the given dimensions.
// Width and height can be changed between frames by creating a new Encoder.
func NewEncoder(width, height int) *Encoder {
	e := &Encoder{
		width:  width,
		height: height,
		active: make([]int, 0, MaxColors),
		outBuf: make([]byte, 0, width*height*2),
	}
	for i := range e.scatter {
		e.scatter[i] = make([]byte, width)
	}
	return e
}
