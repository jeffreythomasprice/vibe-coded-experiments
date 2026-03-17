package main

// appendRLE appends RLE-encoded sixel data for a single color's band row
// to dst. Each byte in data is a 6-bit mask (0–63) that gets offset by '?'
// (0x3F) to produce the sixel character. Runs of identical values are
// compressed as !N<char>. Trailing zeros are trimmed.
func appendRLE(dst []byte, data []byte) []byte {
	n := len(data)
	if n == 0 {
		return dst
	}

	// Trim trailing zeros (sixel char '?' = 0x3F = data byte 0).
	last := n - 1
	for last > 0 && data[last] == 0 {
		last--
	}
	// If everything is zero, emit a single '?' to maintain positioning,
	// but the caller typically skips empty colors entirely.
	if last == 0 && data[0] == 0 {
		return append(dst, '?')
	}
	data = data[:last+1]

	i := 0
	for i < len(data) {
		ch := data[i] + '?'
		run := 1
		for i+run < len(data) && data[i+run] == data[i] {
			run++
		}
		if run >= 4 {
			dst = appendInt(dst, '!', run)
			dst = append(dst, ch)
		} else {
			for range run {
				dst = append(dst, ch)
			}
		}
		i += run
	}
	return dst
}

// appendInt appends prefix followed by the decimal representation of v.
// Used for RLE counts and color indices. Avoids fmt.Sprintf in the hot path.
func appendInt(dst []byte, prefix byte, v int) []byte {
	dst = append(dst, prefix)
	return appendDecimal(dst, v)
}

// appendDecimal appends the decimal string of a non-negative integer to dst.
func appendDecimal(dst []byte, v int) []byte {
	if v == 0 {
		return append(dst, '0')
	}
	// Max digits for int: 20 (but our values are ≤ 1920 or 256)
	var buf [8]byte
	pos := len(buf)
	for v > 0 {
		pos--
		buf[pos] = byte(v%10) + '0'
		v /= 10
	}
	return append(dst, buf[pos:]...)
}
