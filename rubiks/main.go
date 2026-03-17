package main

import (
	"fmt"
	"math"
	"os"
	"time"

	"experiment/graphics"
	"golang.org/x/term"
)

const (
	escapeKeycode = 27

	hideCursor = "\x1b[?25l"
	showCursor = "\x1b[?25h"
	cursorHome = "\x1b[H"

	// Kitty keyboard protocol: push mode with flags 1|2 (disambiguate + report event types)
	kittyEnable  = "\x1b[>3u"
	kittyDisable = "\x1b[<u"
)

func run() error {
	ts, err := GetTermSize(os.Stdout.Fd())
	if err != nil {
		return fmt.Errorf("getting terminal size: %w", err)
	}
	pixelW, pixelH := ts.PixelSize()

	oldState, err := term.MakeRaw(int(os.Stdin.Fd()))
	if err != nil {
		return fmt.Errorf("entering raw mode: %w", err)
	}
	defer term.Restore(int(os.Stdin.Fd()), oldState)

	// Hide cursor and enable Kitty keyboard protocol
	os.Stdout.WriteString(hideCursor + kittyEnable)
	defer os.Stdout.WriteString(kittyDisable + showCursor)

	// Non-blocking keyboard input via goroutine
	quit := make(chan struct{})
	go func() {
		buf := make([]byte, 256)
		for {
			n, err := os.Stdin.Read(buf)
			if err != nil {
				close(quit)
				return
			}
			data := buf[:n]
			for len(data) > 0 {
				evt, consumed, ok := ParseKeyEvent(data)
				if !ok {
					if data[0] == escapeKeycode && n == 1 {
						close(quit)
						return
					}
					data = data[1:]
					continue
				}
				data = data[consumed:]
				if evt.Keycode == escapeKeycode && evt.EventType == Release {
					close(quit)
					return
				}
			}
		}
	}()

	fb := graphics.NewFramebuffer(pixelW, pixelH)

	palette := Palette{Size: 4}
	palette.Colors[0] = [3]byte{0, 0, 0}       // background (black)
	palette.Colors[1] = [3]byte{255, 0, 0}     // red
	palette.Colors[2] = [3]byte{0, 255, 0}     // green
	palette.Colors[3] = [3]byte{0, 100, 255}   // blue

	triangles := []Triangle{
		{
			P0:       graphics.Vec3{X: 0, Y: 0.8, Z: 0},
			P1:       graphics.Vec3{X: -0.8, Y: -0.6, Z: 0},
			P2:       graphics.Vec3{X: 0.8, Y: -0.6, Z: 0},
			ColorIdx: 1,
		},
	}

	aspect := float64(pixelW) / float64(pixelH)
	proj := graphics.Perspective(math.Pi/4, aspect, 0.1, 100)
	view := graphics.LookAt(
		graphics.Vec3{X: 0, Y: 0, Z: 3},
		graphics.Vec3{X: 0, Y: 0, Z: 0},
		graphics.Vec3{X: 0, Y: 1, Z: 0},
	)

	enc := NewEncoder(pixelW, pixelH)
	startTime := time.Now()
	const frameDuration = time.Second / 30

	for {
		select {
		case <-quit:
			return nil
		default:
		}

		angle := time.Since(startTime).Seconds() * math.Pi / 4 // 45°/s
		model := graphics.RotateY(angle)
		mvp := proj.Mul(view).Mul(model)

		os.Stdout.WriteString(cursorHome)
		if err := enc.RenderFrame(os.Stdout, fb, triangles, mvp, palette); err != nil {
			return fmt.Errorf("encoding sixel: %w", err)
		}

		time.Sleep(frameDuration)
	}
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}
