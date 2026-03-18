package main

import (
	"flag"
	"fmt"
	"log/slog"
	"math"
	"os"
	"time"

	"experiment/graphics"
	"experiment/puzzle"
	"experiment/puzzle/solver"

	"golang.org/x/term"
)

const (
	escapeKeycode = 27

	hideCursor = "\x1b[?25l"
	showCursor = "\x1b[?25h"
	cursorHome = "\x1b[H"

	kittyEnable  = "\x1b[>3u"
	kittyDisable = "\x1b[<u"
)

func run() error {
	sizeX := flag.Int("size-x", 3, "number of cubies along X axis (2-10)")
	sizeY := flag.Int("size-y", 3, "number of cubies along Y axis (2-10)")
	sizeZ := flag.Int("size-z", 3, "number of cubies along Z axis (2-10)")
	flag.Parse()

	for _, dim := range []struct {
		name  string
		value int
	}{
		{"size-x", *sizeX},
		{"size-y", *sizeY},
		{"size-z", *sizeZ},
	} {
		if dim.value < 2 || dim.value > 10 {
			return fmt.Errorf("-%s must be between 2 and 10, got %d", dim.name, dim.value)
		}
	}

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

	os.Stdout.WriteString(hideCursor + kittyEnable)
	defer os.Stdout.WriteString(kittyDisable + showCursor)

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

	if err := os.MkdirAll("logs", 0o755); err != nil {
		return fmt.Errorf("creating logs directory: %w", err)
	}
	logFile, err := os.Create("logs/rubiks.log")
	if err != nil {
		return fmt.Errorf("creating log file: %w", err)
	}
	defer logFile.Close()
	logger := slog.New(slog.NewTextHandler(logFile, nil))
	solver.SetLogWriter(logFile)

	p := puzzle.NewPuzzle(*sizeX, *sizeY, *sizeZ)

	fb := graphics.NewFramebuffer(pixelW, pixelH)

	palette := Palette{Size: 8}
	palette.Colors[0] = [3]byte{0, 0, 0}       // Black
	palette.Colors[1] = [3]byte{255, 255, 255}  // White
	palette.Colors[2] = [3]byte{255, 0, 0}      // Red
	palette.Colors[3] = [3]byte{255, 165, 0}    // Orange
	palette.Colors[4] = [3]byte{0, 0, 255}      // Blue
	palette.Colors[5] = [3]byte{0, 200, 0}      // Green
	palette.Colors[6] = [3]byte{255, 255, 0}    // Yellow
	palette.Colors[7] = [3]byte{40, 40, 40}     // DarkGray

	aspect := float64(pixelW) / float64(pixelH)
	proj := graphics.Perspective(math.Pi/4, aspect, 0.1, 100)
	maxDim := float64(max(*sizeX, *sizeY, *sizeZ))
	eye := graphics.Vec3{X: 0, Y: maxDim * 1.5, Z: maxDim * 2.5}
	view := graphics.LookAt(
		eye,
		graphics.Vec3{},
		graphics.Vec3{X: 0, Y: 1, Z: 0},
	)

	enc := NewEncoder(pixelW, pixelH)

	state := puzzle.NewDemoState(p)
	state.SetLogger(logger)
	var anim *puzzle.Animation

	lastFrame := time.Now()
	const frameDuration = time.Second / 30

	for {
		select {
		case <-quit:
			return nil
		default:
		}

		now := time.Now()
		dt := now.Sub(lastFrame).Seconds()
		lastFrame = now

		// Gentle orbit rotation
		orbitAngle := now.Sub(time.Time{}).Seconds() * math.Pi / 8

		if anim == nil {
			if state.NeedsSolve() {
				logger.Info("searching for solution")
				solveStart := time.Now()
				moves, err := solver.Solve(p)
				if err != nil {
					return fmt.Errorf("solving puzzle: %w", err)
				}
				logger.Info("solution found", "moves", len(moves), "elapsed", time.Since(solveStart))
				sliceMoves := make([]puzzle.SliceMove, len(moves))
				for i, m := range moves {
					sliceMoves[i] = puzzle.SliceMove{Axis: m.Axis, Slice: m.Slice, Turns: m.Turns}
				}
				state.SetSolveMoves(sliceMoves)
			}
			anim = state.NextAnimation(p, dt)
		}

		var triangles []graphics.Triangle

		if anim != nil {
			anim.Elapsed += dt

			stationary, rotating := anim.SplitTriangles(p)
			angle := anim.CurrentAngle()
			rotMat := puzzle.RotateAxis(anim.Axis, angle)

			for i := range rotating {
				rotating[i].P0 = rotMat.TransformPoint(rotating[i].P0)
				rotating[i].P1 = rotMat.TransformPoint(rotating[i].P1)
				rotating[i].P2 = rotMat.TransformPoint(rotating[i].P2)
			}

			triangles = append(stationary, rotating...)

			if anim.Done() {
				anim.ApplyTurn(p)
				state.AnimationDone()
				anim = nil
			}
		} else {
			triangles = puzzle.GenerateTriangles(p, 0.05)
		}

		model := graphics.RotateY(orbitAngle)
		mvp := proj.Mul(view).Mul(model)

		os.Stdout.WriteString(cursorHome)
		if err := enc.RenderFrame(os.Stdout, fb, triangles, mvp, palette); err != nil {
			return fmt.Errorf("encoding sixel: %w", err)
		}

		elapsed := time.Since(now)
		if elapsed < frameDuration {
			time.Sleep(frameDuration - elapsed)
		}
	}
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}
