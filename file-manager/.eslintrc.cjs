module.exports = {
  root: true,
  parser: "@typescript-eslint/parser",
  parserOptions: { project: true },
  plugins: ["@typescript-eslint"],
  extends: [
    "eslint:recommended",
    "plugin:@typescript-eslint/recommended-type-checked",
    "plugin:@typescript-eslint/strict-type-checked",
    "prettier",
  ],
  rules: {
    "@typescript-eslint/no-floating-promises": "error",
    "@typescript-eslint/await-thenable": "error",
    "@typescript-eslint/prefer-nullish-coalescing": "error",
    // Enforce async/await; ban .then()/.catch() chains
    "@typescript-eslint/no-misused-promises": [
      "error",
      { checksVoidReturn: true },
    ],
  },
};
