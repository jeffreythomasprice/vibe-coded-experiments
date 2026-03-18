package solver

import (
	"io"
	"log/slog"
)

var logger = slog.New(slog.NewTextHandler(io.Discard, nil))

// SetLogWriter enables logging to the given writer.
func SetLogWriter(w io.Writer) {
	logger = slog.New(slog.NewTextHandler(w, nil))
}
