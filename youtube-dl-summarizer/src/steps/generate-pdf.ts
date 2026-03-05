import { join } from "node:path";
import { unlink } from "node:fs/promises";
import { marked } from "marked";
import { CACHE_FILES, cacheExists, cacheRead } from "../cache.ts";
import { ensureBinary, run } from "../utils/subprocess.ts";

/**
 * Generate a PDF from the markdown summary, embedding snapshot images as base64 data URIs.
 */
export async function generatePdf(
  cacheDir: string,
  verbose: boolean,
): Promise<string> {
  const pdfPath = join(cacheDir, CACHE_FILES.summaryPdf);

  if (await cacheExists(cacheDir, CACHE_FILES.summaryPdf)) {
    console.error("Generating PDF... cached, skipping");
    return pdfPath;
  }

  console.error("Generating PDF...");
  ensureBinary("wkhtmltopdf", "sudo apt install wkhtmltopdf");

  // Prefer enriched summary with snapshots, fall back to plain summary
  const hasEnriched = await cacheExists(cacheDir, CACHE_FILES.summaryWithSnapshots);
  const markdown = await cacheRead(
    cacheDir,
    hasEnriched ? CACHE_FILES.summaryWithSnapshots : CACHE_FILES.summary,
  );

  // Replace relative image paths with base64 data URIs
  const withEmbeddedImages = await embedImages(markdown, cacheDir, verbose);

  // Convert markdown to HTML
  const htmlBody = await marked(withEmbeddedImages);

  const html = `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
      font-size: 14px;
      line-height: 1.6;
      color: #24292e;
      max-width: 100%;
      padding: 0;
    }
    h1 { font-size: 24px; border-bottom: 1px solid #eaecef; padding-bottom: 8px; }
    h2 { font-size: 20px; border-bottom: 1px solid #eaecef; padding-bottom: 6px; }
    h3 { font-size: 16px; }
    img { max-width: 100%; height: auto; border-radius: 4px; margin: 8px 0; }
    code { background: #f6f8fa; padding: 2px 6px; border-radius: 3px; font-size: 13px; }
    pre { background: #f6f8fa; padding: 12px; border-radius: 6px; overflow-x: auto; }
    pre code { background: none; padding: 0; }
    blockquote { border-left: 4px solid #dfe2e5; margin: 0; padding: 0 16px; color: #6a737d; }
    ul, ol { padding-left: 24px; }
    li { margin: 4px 0; }
  </style>
</head>
<body>${htmlBody}</body>
</html>`;

  // Write HTML to a temp file for wkhtmltopdf
  const htmlPath = join(cacheDir, "summary.html");
  await Bun.write(htmlPath, html);

  try {
    await run([
      "wkhtmltopdf",
      "--page-size", "A4",
      "--margin-top", "20mm",
      "--margin-bottom", "20mm",
      "--margin-left", "15mm",
      "--margin-right", "15mm",
      "--enable-local-file-access",
      htmlPath,
      pdfPath,
    ]);
  } finally {
    await unlink(htmlPath).catch(() => {});
  }

  if (verbose) console.error(`[generate-pdf] wrote ${pdfPath}`);
  return pdfPath;
}

async function embedImages(
  markdown: string,
  cacheDir: string,
  verbose: boolean,
): Promise<string> {
  const imageRegex = /!\[([^\]]*)\]\(([^)]+)\)/g;
  let result = markdown;

  for (const match of markdown.matchAll(imageRegex)) {
    const [fullMatch, alt, imagePath] = match;
    const absPath = imagePath.startsWith("/") ? imagePath : join(cacheDir, imagePath);
    const file = Bun.file(absPath);

    if (await file.exists()) {
      const buffer = await file.arrayBuffer();
      const base64 = Buffer.from(buffer).toString("base64");
      const mimeType = imagePath.endsWith(".png") ? "image/png" : "image/jpeg";
      const dataUri = `data:${mimeType};base64,${base64}`;
      result = result.replace(fullMatch, `![${alt}](${dataUri})`);
      if (verbose) console.error(`[generate-pdf] embedded ${imagePath}`);
    }
  }

  return result;
}
