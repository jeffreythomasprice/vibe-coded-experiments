import pdf from "pdf-parse";
import logger from "./logger.js";
import { CHUNK_SIZE, CHUNK_OVERLAP } from "./config.js";

export interface ExtractResult {
  text: string;
  pageBoundaries?: number[];
}

export interface ChunkWithOffset {
  text: string;
  startOffset: number;
  endOffset: number;
}

export async function extractText(filepath: string): Promise<ExtractResult> {
  logger.debug({ filepath }, "extracting text");
  const ext = filepath.split(".").pop()?.toLowerCase();
  if (ext === "pdf") {
    return extractPdf(filepath);
  }
  return { text: await Bun.file(filepath).text() };
}

async function extractPdf(filepath: string): Promise<ExtractResult> {
  const buffer = Buffer.from(await Bun.file(filepath).arrayBuffer());
  const pageBoundaries: number[] = [];
  let offset = 2; // pdf-parse prepends "\n\n" before first page

  const data = await pdf(buffer, {
    pagerender: (pageData: any) => {
      return pageData.getTextContent().then((textContent: any) => {
        pageBoundaries.push(offset);
        const pageText = textContent.items.map((item: any) => item.str).join("");
        offset += pageText.length + 2; // +2 for "\n\n" separator
        return pageText;
      });
    },
  });

  return { text: data.text, pageBoundaries };
}

export function mapChunksToPages(
  chunks: ChunkWithOffset[],
  pageBoundaries: number[],
): { pageStart: number | null; pageEnd: number | null }[] {
  return chunks.map((chunk) => {
    let startPage = 1;
    for (let i = pageBoundaries.length - 1; i >= 0; i--) {
      if (chunk.startOffset >= pageBoundaries[i]) {
        startPage = i + 1;
        break;
      }
    }

    let endPage = startPage;
    for (let i = pageBoundaries.length - 1; i >= 0; i--) {
      if (chunk.endOffset - 1 >= pageBoundaries[i]) {
        endPage = i + 1;
        break;
      }
    }

    return { pageStart: startPage, pageEnd: endPage };
  });
}

const SEPARATORS = ["\n\n", "\n", " ", ""];

export function chunkText(
  text: string,
  chunkSize: number = CHUNK_SIZE,
  overlap: number = CHUNK_OVERLAP,
): ChunkWithOffset[] {
  return recursiveSplit(text, 0, chunkSize, overlap, SEPARATORS);
}

function recursiveSplit(
  text: string,
  baseOffset: number,
  chunkSize: number,
  overlap: number,
  separators: string[],
): ChunkWithOffset[] {
  if (text.length <= chunkSize) {
    const trimmed = text.trim();
    if (!trimmed) return [];
    const trimStart = text.indexOf(trimmed);
    return [{
      text: trimmed,
      startOffset: baseOffset + trimStart,
      endOffset: baseOffset + trimStart + trimmed.length,
    }];
  }

  const separator = separators[0] ?? "";
  const nextSeparators = separators.length > 1 ? separators.slice(1) : separators;

  // Split by separator, tracking offsets
  const pieces: { text: string; offset: number }[] = [];
  if (separator === "") {
    // Character-level split: just force chunks at chunkSize boundaries
    for (let i = 0; i < text.length; i += chunkSize) {
      pieces.push({ text: text.slice(i, i + chunkSize), offset: baseOffset + i });
    }
    return pieces.filter(p => p.text.trim()).map(p => {
      const trimmed = p.text.trim();
      const trimStart = p.text.indexOf(trimmed);
      return {
        text: trimmed,
        startOffset: p.offset + trimStart,
        endOffset: p.offset + trimStart + trimmed.length,
      };
    });
  }

  let pos = 0;
  while (pos < text.length) {
    const idx = text.indexOf(separator, pos);
    if (idx === -1) {
      pieces.push({ text: text.slice(pos), offset: baseOffset + pos });
      break;
    }
    pieces.push({ text: text.slice(pos, idx), offset: baseOffset + pos });
    pos = idx + separator.length;
  }

  // Merge pieces into chunks respecting chunkSize, with overlap
  const results: ChunkWithOffset[] = [];
  let current = "";
  let currentOffset = pieces.length > 0 ? pieces[0].offset : baseOffset;

  for (let i = 0; i < pieces.length; i++) {
    const piece = pieces[i];
    const wouldBe = current
      ? current + separator + piece.text
      : piece.text;

    if (wouldBe.length <= chunkSize) {
      if (!current) {
        current = piece.text;
        currentOffset = piece.offset;
      } else {
        current = wouldBe;
      }
    } else {
      // Flush current
      if (current) {
        if (current.length > chunkSize) {
          results.push(...recursiveSplit(current, currentOffset, chunkSize, overlap, nextSeparators));
        } else {
          const trimmed = current.trim();
          if (trimmed) {
            const trimStart = current.indexOf(trimmed);
            results.push({
              text: trimmed,
              startOffset: currentOffset + trimStart,
              endOffset: currentOffset + trimStart + trimmed.length,
            });
          }
        }
      }

      // Handle overlap: walk back from end of flushed chunk to find overlap text
      if (overlap > 0 && current) {
        const overlapText = current.slice(-overlap);
        const overlapOffset = currentOffset + current.length - overlapText.length;
        current = overlapText + separator + piece.text;
        currentOffset = overlapOffset;
      } else {
        current = piece.text;
        currentOffset = piece.offset;
      }

      // If this single piece is already too big, it'll be split on next flush
    }
  }

  // Flush remaining
  if (current) {
    if (current.length > chunkSize) {
      results.push(...recursiveSplit(current, currentOffset, chunkSize, overlap, nextSeparators));
    } else {
      const trimmed = current.trim();
      if (trimmed) {
        const trimStart = current.indexOf(trimmed);
        results.push({
          text: trimmed,
          startOffset: currentOffset + trimStart,
          endOffset: currentOffset + trimStart + trimmed.length,
        });
      }
    }
  }

  return results;
}
