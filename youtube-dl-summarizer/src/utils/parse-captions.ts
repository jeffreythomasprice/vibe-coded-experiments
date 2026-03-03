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
