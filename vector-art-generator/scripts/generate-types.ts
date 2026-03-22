import { readdir, readFile, writeFile, mkdir } from "fs/promises";
import { join, basename } from "path";
import { compile } from "json-schema-to-typescript";

const SCHEMAS_DIR = join(import.meta.dir, "..", "schemas");
const OUT_DIR = join(import.meta.dir, "..", "src", "generated");

async function generateTypes() {
  await mkdir(OUT_DIR, { recursive: true });

  const files = (await readdir(SCHEMAS_DIR)).filter((f) => f.endsWith(".json"));
  const types: string[] = [];
  const validatorFns: string[] = [];
  const validatorImports: string[] = [];

  for (const file of files) {
    const schemaPath = join(SCHEMAS_DIR, file);
    const schemaText = await readFile(schemaPath, "utf-8");
    const schema = JSON.parse(schemaText);

    const ts = await compile(schema, schema.title, {
      bannerComment: "",
      additionalProperties: false,
      format: false,
    });
    types.push(ts);

    const typeName = schema.title;
    const fnName = `is${typeName}`;
    validatorImports.push(typeName);

    validatorFns.push(`
const ${typeName}Schema = ${schemaText};

export function ${fnName}(data: unknown): data is ${typeName} {
  return validate(${typeName}Schema, data);
}

export function assert${typeName}(data: unknown): asserts data is ${typeName} {
  if (!${fnName}(data)) {
    throw new Error("Invalid ${typeName}");
  }
}
`);
  }

  // Write types.ts
  const typesContent = `// Generated from schemas/ — do not edit manually\n\n${types.join("\n")}`;
  await writeFile(join(OUT_DIR, "types.ts"), typesContent);

  // Write validators.ts
  const validatorsContent = `// Generated from schemas/ — do not edit manually
import Ajv from "ajv";
import type { ${validatorImports.join(", ")} } from "./types";

const ajv = new Ajv();

function validate(schema: object, data: unknown): boolean {
  const valid = ajv.validate(schema, data);
  return !!valid;
}
${validatorFns.join("\n")}`;
  await writeFile(join(OUT_DIR, "validators.ts"), validatorsContent);

  // Write index.ts
  await writeFile(
    join(OUT_DIR, "index.ts"),
    `export * from "./types";\nexport * from "./validators";\n`
  );

  console.log(
    `Generated types and validators for ${files.length} schemas in ${OUT_DIR}`
  );
}

generateTypes();
