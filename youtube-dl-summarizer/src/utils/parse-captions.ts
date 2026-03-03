/**
 * Convert seconds to [HH:MM:SS] format.
 */
function formatTimestamp(totalSeconds: number): string {
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = Math.floor(totalSeconds % 60);
  return `[${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}]`;
}

/**
 * Parse a VTT/SRT timestamp line and return start time in seconds.
 */
function parseTimestampLine(line: string): number | null {
  const match = line.match(/^(\d{2}):(\d{2})[:.]([\d.]+)\s*-->/);
  if (!match) return null;
  return parseInt(match[1]) * 3600 + parseInt(match[2]) * 60 + parseFloat(match[3]);
}

/**
 * Parse VTT/SRT caption files into clean plain text.
 */
export function parseCaptions(content: string): string {
  const lines = content.split("\n");
  const textLines: string[] = [];
  let lastLine = "";

  for (const raw of lines) {
    const line = raw.trim();

    // Skip WEBVTT header and metadata
    if (line.startsWith("WEBVTT") || line.startsWith("Kind:") || line.startsWith("Language:")) {
      continue;
    }

    // Skip SRT sequence numbers (bare integers)
    if (/^\d+$/.test(line)) continue;

    // Skip timestamp lines (00:00:00.000 --> 00:00:00.000)
    if (/^\d{2}:\d{2}[:.,][\d.,]+ --> \d{2}:\d{2}[:.,][\d.,]+/.test(line)) continue;

    // Skip empty lines
    if (!line) continue;

    // Strip HTML tags (e.g. <c>, </c>, <00:00:01.234>)
    const cleaned = line.replace(/<[^>]+>/g, "").trim();
    if (!cleaned) continue;

    // Deduplicate consecutive identical lines
    if (cleaned === lastLine) continue;

    textLines.push(cleaned);
    lastLine = cleaned;
  }

  return textLines.join("\n");
}

/**
 * Parse VTT/SRT caption files into text with [HH:MM:SS] timestamp prefixes.
 * Only emits a timestamp when the time changes (avoids repetition for deduped lines).
 */
export function parseCaptionsWithTimestamps(content: string): string {
  const lines = content.split("\n");
  const textLines: string[] = [];
  let lastLine = "";
  let currentTimestamp: string | null = null;
  let lastEmittedTimestamp: string | null = null;

  for (const raw of lines) {
    const line = raw.trim();

    // Skip WEBVTT header and metadata
    if (line.startsWith("WEBVTT") || line.startsWith("Kind:") || line.startsWith("Language:")) {
      continue;
    }

    // Skip SRT sequence numbers (bare integers)
    if (/^\d+$/.test(line)) continue;

    // Parse timestamp lines
    const startSeconds = parseTimestampLine(line);
    if (startSeconds !== null) {
      currentTimestamp = formatTimestamp(startSeconds);
      continue;
    }

    // Skip empty lines
    if (!line) continue;

    // Strip HTML tags
    const cleaned = line.replace(/<[^>]+>/g, "").trim();
    if (!cleaned) continue;

    // Deduplicate consecutive identical lines
    if (cleaned === lastLine) continue;

    // Prefix with timestamp if it changed since last emitted line
    if (currentTimestamp && currentTimestamp !== lastEmittedTimestamp) {
      textLines.push(`${currentTimestamp} ${cleaned}`);
      lastEmittedTimestamp = currentTimestamp;
    } else {
      textLines.push(cleaned);
    }

    lastLine = cleaned;
  }

  return textLines.join("\n");
}
