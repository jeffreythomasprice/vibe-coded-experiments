#!/usr/bin/env bun
/**
 * Codegen script: reads schemas/*.json, writes three output kinds into src/generated/:
 *   types/      — TypeScript interfaces (via json-schema-to-typescript)
 *   validators/ — typed AJV wrappers: isX() / assertX()
 *   schemas/    — Fastify-compatible schema const exports
 *
 * Run: bun run --cwd packages/schemas generate
 */
import { compile } from "json-schema-to-typescript";
import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const ROOT = join(__dirname, "..");
const SCHEMAS_DIR = join(ROOT, "schemas");
const GENERATED_DIR = join(ROOT, "src", "generated");
const TYPES_DIR = join(GENERATED_DIR, "types");
const VALIDATORS_DIR = join(GENERATED_DIR, "validators");
const SCHEMAS_EXPORT_DIR = join(GENERATED_DIR, "schemas");

const BANNER =
    "// GENERATED FILE — DO NOT EDIT MANUALLY\n" +
    "// Run `bun run --cwd packages/schemas generate` to regenerate\n";

function toPascalCase(kebab: string): string {
    return kebab
        .split("-")
        .map((s) => (s.length > 0 ? s[0]!.toUpperCase() + s.slice(1) : ""))
        .join("");
}

function toCamelCase(kebab: string): string {
    const pascal = toPascalCase(kebab);
    return pascal.length > 0 ? pascal[0]!.toLowerCase() + pascal.slice(1) : "";
}

interface SchemaKey {
    kebab: string;
    camel: string;
    pascal: string;
}

async function main(): Promise<void> {
    await mkdir(TYPES_DIR, { recursive: true });
    await mkdir(VALIDATORS_DIR, { recursive: true });
    await mkdir(SCHEMAS_EXPORT_DIR, { recursive: true });

    const allFiles = await readdir(SCHEMAS_DIR);
    const files = allFiles.filter((f) => f.endsWith(".json"));
    const keys: SchemaKey[] = [];

    for (const file of files) {
        const kebab = basename(file, ".json");
        const pascal = toPascalCase(kebab);
        const camel = toCamelCase(kebab);
        keys.push({ kebab, camel, pascal });

        const schemaPath = join(SCHEMAS_DIR, file);
        const schemaText = await readFile(schemaPath, "utf-8");
        // JSON.parse returns any; cast to object for downstream use
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
        const schema = JSON.parse(schemaText);

        // 1. Generate TypeScript types via json-schema-to-typescript
        // eslint-disable-next-line @typescript-eslint/no-unsafe-argument
        const tsTypes = await compile(schema, pascal, {
            bannerComment: BANNER,
            format: false,
        });
        await writeFile(join(TYPES_DIR, `${kebab}.ts`), tsTypes);

        // 2. Generate typed AJV validator functions
        const inlinedSchema = JSON.stringify(schema, null, 2);
        const validatorContent = [
            BANNER,
            `import Ajv from "ajv";`,
            `import type { ${pascal} } from "../types/${kebab}.js";`,
            ``,
            `// eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-explicit-any`,
            `const _validate = new Ajv().compile(${inlinedSchema} as any);`,
            ``,
            `export function is${pascal}(data: unknown): data is ${pascal} {`,
            `    return _validate(data) as boolean;`,
            `}`,
            ``,
            `export function assert${pascal}(data: unknown): asserts data is ${pascal} {`,
            `    if (!is${pascal}(data)) {`,
            `        throw new Error(\`Validation failed: \${JSON.stringify(_validate.errors)}\`);`,
            `    }`,
            `}`,
            ``,
        ].join("\n");
        await writeFile(join(VALIDATORS_DIR, `${kebab}.ts`), validatorContent);

        // 3. Generate Fastify-compatible schema const export
        const schemaExportContent = [
            BANNER,
            `export const ${camel}Schema = ${inlinedSchema} as const;`,
            ``,
        ].join("\n");
        await writeFile(join(SCHEMAS_EXPORT_DIR, `${kebab}.ts`), schemaExportContent);

        console.log(`✓ Generated ${pascal}`);
    }

    // 4. Write generated barrel index
    const indexLines: string[] = [BANNER];
    for (const { kebab, pascal } of keys) {
        indexLines.push(`export type { ${pascal} } from "./types/${kebab}.js";`);
    }
    for (const { kebab, pascal } of keys) {
        indexLines.push(`export { is${pascal}, assert${pascal} } from "./validators/${kebab}.js";`);
    }
    for (const { kebab, camel } of keys) {
        indexLines.push(`export { ${camel}Schema } from "./schemas/${kebab}.js";`);
    }
    indexLines.push("");
    await writeFile(join(GENERATED_DIR, "index.ts"), indexLines.join("\n"));

    console.log("✓ Generated barrel index");
}

main().catch((err: unknown) => {
    console.error(err);
    process.exit(1);
});
