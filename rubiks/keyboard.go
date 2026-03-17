package main

// EventType represents whether a key was pressed, repeated, or released.
type EventType int

const (
	Press   EventType = 1
	Repeat  EventType = 2
	Release EventType = 3
)

// KeyEvent represents a parsed Kitty keyboard protocol CSI u sequence.
type KeyEvent struct {
	Keycode   int
	Modifiers int // 0 = none (protocol sends mod+1, we subtract)
	EventType EventType
}

// ParseKeyEvent parses one CSI u sequence from buf.
// Format: ESC [ keycode ; modifiers:event_type u
// Returns the event, bytes consumed, and whether parsing succeeded.
// ok is false if the buffer is incomplete or doesn't contain a key CSI u sequence.
func ParseKeyEvent(buf []byte) (KeyEvent, int, bool) {
	if len(buf) < 3 {
		return KeyEvent{}, 0, false
	}

	// Must start with ESC [
	if buf[0] != 0x1b || buf[1] != '[' {
		return KeyEvent{}, 0, false
	}

	// Find the terminator 'u'
	end := -1
	for i := 2; i < len(buf); i++ {
		if buf[i] == 'u' {
			end = i
			break
		}
		// If we hit another ESC, the sequence is malformed/not a CSI u
		if buf[i] == 0x1b {
			return KeyEvent{}, 0, false
		}
		// If we hit a letter that isn't 'u', this is a different CSI sequence
		if buf[i] >= 'A' && buf[i] <= 'z' && buf[i] != 'u' {
			return KeyEvent{}, 0, false
		}
	}

	if end == -1 {
		// Incomplete sequence
		return KeyEvent{}, 0, false
	}

	// Parse: keycode [; modifiers [:event_type]] u
	params := buf[2:end]

	keycode := 0
	modifiers := 0
	eventType := Press // default

	// Split on ';'
	semi := -1
	for i, b := range params {
		if b == ';' {
			semi = i
			break
		}
	}

	if semi == -1 {
		// Just keycode, no modifiers
		var ok bool
		keycode, ok = parseDecimal(params)
		if !ok {
			return KeyEvent{}, 0, false
		}
	} else {
		var ok bool
		keycode, ok = parseDecimal(params[:semi])
		if !ok {
			return KeyEvent{}, 0, false
		}

		modPart := params[semi+1:]

		// Split modPart on ':'
		colon := -1
		for i, b := range modPart {
			if b == ':' {
				colon = i
				break
			}
		}

		if colon == -1 {
			// Just modifiers, no event type
			modifiers, ok = parseDecimal(modPart)
			if !ok {
				return KeyEvent{}, 0, false
			}
			modifiers-- // protocol sends mod+1
		} else {
			modifiers, ok = parseDecimal(modPart[:colon])
			if !ok {
				return KeyEvent{}, 0, false
			}
			modifiers-- // protocol sends mod+1

			et, ok := parseDecimal(modPart[colon+1:])
			if !ok {
				return KeyEvent{}, 0, false
			}
			eventType = EventType(et)
		}
	}

	return KeyEvent{
		Keycode:   keycode,
		Modifiers: modifiers,
		EventType: eventType,
	}, end + 1, true
}

func parseDecimal(b []byte) (int, bool) {
	if len(b) == 0 {
		return 0, false
	}
	n := 0
	for _, c := range b {
		if c < '0' || c > '9' {
			return 0, false
		}
		n = n*10 + int(c-'0')
	}
	return n, true
}
