/**
 * Codegen script: reads JSON Schema files from ../schemas/ and generates:
 *   - src/generated/types.ts  (TypeScript interfaces)
 *   - src/generated/schemas.ts (schema objects as TS constants)
 */
import { readdir, readFile, mkdir, writeFile } from "fs/promises";
import { join, resolve } from "path";
import { compile } from "json-schema-to-typescript";

const BANNER = "// AUTO-GENERATED — do not edit. Run `bun run generate` to regenerate.\n\n";

const schemasDir = resolve(import.meta.dir, "../schemas");
const outDir = resolve(import.meta.dir, "../src/generated");

function schemaNameToIdentifier(filename: string): string {
  // chunk-result.json → ChunkResult
  return filename
    .replace(/\.json$/, "")
    .split("-")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("");
}

async function main() {
  await mkdir(outDir, { recursive: true });

  const files = (await readdir(schemasDir)).filter((f) => f.endsWith(".json")).sort();

  // --- Generate types.ts ---
  // Compile each schema, then post-process to:
  //   1. Strip "Json" suffix from all type/interface names (caused by .json in $id)
  //   2. Deduplicate interfaces that appear via $ref resolution
  const seen = new Set<string>();
  let typesContent = BANNER;

  for (const file of files) {
    const schemaText = await readFile(join(schemasDir, file), "utf-8");
    const schema = JSON.parse(schemaText);
    let ts = await compile(schema, schemaNameToIdentifier(file), {
      cwd: schemasDir,
      bannerComment: "",
      additionalProperties: false,
    });

    // Strip "Json" suffix from interface names and type references
    ts = ts.replace(/\b(\w+)Json\b/g, "$1");

    // Split into individual interface blocks and deduplicate
    const blocks = ts.split(/(?=export interface )/).filter((b) => b.trim());
    for (const block of blocks) {
      const match = block.match(/^export interface (\w+)/);
      if (match) {
        if (seen.has(match[1])) continue;
        seen.add(match[1]);
      }
      typesContent += block;
    }
    typesContent += "\n";
  }

  await writeFile(join(outDir, "types.ts"), typesContent);
  console.log("Generated src/generated/types.ts");

  // --- Generate schemas.ts ---
  let schemasContent = BANNER;
  for (const file of files) {
    const schemaText = await readFile(join(schemasDir, file), "utf-8");
    const name = schemaNameToIdentifier(file);
    schemasContent += `export const ${name}Schema = ${schemaText.trim()} as const;\n\n`;
  }
  await writeFile(join(outDir, "schemas.ts"), schemasContent);
  console.log("Generated src/generated/schemas.ts");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
