package main

import (
	"fmt"
	"syscall"
	"unsafe"
)

// TermSize holds terminal dimensions in both cells and pixels.
type TermSize struct {
	Rows, Cols     uint16
	XPixel, YPixel uint16
}

// GetTermSize queries the terminal for its size using the TIOCGWINSZ ioctl.
func GetTermSize(fd uintptr) (TermSize, error) {
	var ts TermSize
	_, _, errno := syscall.Syscall(
		syscall.SYS_IOCTL,
		fd,
		syscall.TIOCGWINSZ,
		uintptr(unsafe.Pointer(&ts)),
	)
	if errno != 0 {
		return TermSize{}, fmt.Errorf("TIOCGWINSZ ioctl: %w", errno)
	}
	return ts, nil
}

// PixelSize returns the terminal's pixel dimensions.
// If the terminal reports 0 for pixel size, it falls back to cells * 8 (width)
// and cells * 16 (height) as a rough estimate.
func (ts TermSize) PixelSize() (width, height int) {
	width = int(ts.XPixel)
	height = int(ts.YPixel)
	if width == 0 {
		width = int(ts.Cols) * 8
	}
	if height == 0 {
		height = int(ts.Rows) * 16
	}
	return width, height
}
