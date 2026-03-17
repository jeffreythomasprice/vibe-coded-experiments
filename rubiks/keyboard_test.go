package main

import (
	"testing"
)

func TestParseKeyEvent(t *testing.T) {
	tests := []struct {
		name     string
		input    []byte
		wantEvt  KeyEvent
		wantN    int
		wantOK   bool
	}{
		{
			name:    "simple press (no modifiers, no event type)",
			input:   []byte("\x1b[97u"),
			wantEvt: KeyEvent{Keycode: 97, Modifiers: 0, EventType: Press},
			wantN:   5,
			wantOK:  true,
		},
		{
			name:    "escape press",
			input:   []byte("\x1b[27u"),
			wantEvt: KeyEvent{Keycode: 27, Modifiers: 0, EventType: Press},
			wantN:   5,
			wantOK:  true,
		},
		{
			name:    "escape release",
			input:   []byte("\x1b[27;1:3u"),
			wantEvt: KeyEvent{Keycode: 27, Modifiers: 0, EventType: Release},
			wantN:   9,
			wantOK:  true,
		},
		{
			name:    "key repeat",
			input:   []byte("\x1b[97;1:2u"),
			wantEvt: KeyEvent{Keycode: 97, Modifiers: 0, EventType: Repeat},
			wantN:   9,
			wantOK:  true,
		},
		{
			name:    "shift modifier press",
			input:   []byte("\x1b[97;2:1u"),
			wantEvt: KeyEvent{Keycode: 97, Modifiers: 1, EventType: Press},
			wantN:   9,
			wantOK:  true,
		},
		{
			name:    "ctrl+shift release",
			input:   []byte("\x1b[97;6:3u"),
			wantEvt: KeyEvent{Keycode: 97, Modifiers: 5, EventType: Release},
			wantN:   9,
			wantOK:  true,
		},
		{
			name:    "modifiers without event type",
			input:   []byte("\x1b[97;2u"),
			wantEvt: KeyEvent{Keycode: 97, Modifiers: 1, EventType: Press},
			wantN:   7,
			wantOK:  true,
		},
		{
			name:   "incomplete ESC only",
			input:  []byte("\x1b"),
			wantN:  0,
			wantOK: false,
		},
		{
			name:   "incomplete ESC [",
			input:  []byte("\x1b["),
			wantN:  0,
			wantOK: false,
		},
		{
			name:   "incomplete CSI no terminator",
			input:  []byte("\x1b[97"),
			wantN:  0,
			wantOK: false,
		},
		{
			name:   "non-key CSI sequence (cursor position)",
			input:  []byte("\x1b[1;1H"),
			wantN:  0,
			wantOK: false,
		},
		{
			name:   "non-key CSI sequence (arrow key)",
			input:  []byte("\x1b[A"),
			wantN:  0,
			wantOK: false,
		},
		{
			name:    "multiple events in buffer - parses first",
			input:   []byte("\x1b[97;1:1u\x1b[97;1:3u"),
			wantEvt: KeyEvent{Keycode: 97, Modifiers: 0, EventType: Press},
			wantN:   9,
			wantOK:  true,
		},
		{
			name:   "empty buffer",
			input:  []byte{},
			wantN:  0,
			wantOK: false,
		},
		{
			name:   "garbage data",
			input:  []byte("hello"),
			wantN:  0,
			wantOK: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			evt, n, ok := ParseKeyEvent(tt.input)
			if ok != tt.wantOK {
				t.Fatalf("ok = %v, want %v", ok, tt.wantOK)
			}
			if n != tt.wantN {
				t.Fatalf("consumed = %d, want %d", n, tt.wantN)
			}
			if ok && evt != tt.wantEvt {
				t.Fatalf("event = %+v, want %+v", evt, tt.wantEvt)
			}
		})
	}
}

func TestParseKeyEventMultiple(t *testing.T) {
	// Simulate a buffer with two complete events
	buf := []byte("\x1b[97;1:1u\x1b[27;1:3u")

	evt1, n1, ok1 := ParseKeyEvent(buf)
	if !ok1 {
		t.Fatal("failed to parse first event")
	}
	if evt1.Keycode != 97 || evt1.EventType != Press {
		t.Fatalf("first event = %+v, want keycode=97, press", evt1)
	}

	evt2, n2, ok2 := ParseKeyEvent(buf[n1:])
	if !ok2 {
		t.Fatal("failed to parse second event")
	}
	if evt2.Keycode != 27 || evt2.EventType != Release {
		t.Fatalf("second event = %+v, want keycode=27, release", evt2)
	}

	remaining := buf[n1+n2:]
	if len(remaining) != 0 {
		t.Fatalf("expected empty remaining buffer, got %d bytes", len(remaining))
	}
}
