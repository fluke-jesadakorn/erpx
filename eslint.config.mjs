import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  {
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-empty-object-type": "warn",
      "@typescript-eslint/no-unused-vars": [
        "warn",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],
      "@typescript-eslint/no-unused-expressions": "off",
      "@typescript-eslint/no-non-null-assertion": "off",
      "@typescript-eslint/no-require-imports": "warn",
      "react-hooks/exhaustive-deps": "warn",
      "@next/next/no-img-element": "off",
      "react-hooks/set-state-in-effect": "off",
      "react-hooks/purity": "off",
      "react-hooks/refs": "off",
      "react/no-unescaped-entities": "warn",
      "prefer-const": "warn",
      "import/no-anonymous-default-export": "warn",
    },
  },
  globalIgnores([
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
    "lib/**",
    "lib/native/vision-ocr/**",
    "lib/native/vision-ocr/node_modules/**",
    "tests/**",
    "db/**",
    "n8n/**",
    "sample/**",
    "*.config.mjs",
    "*.config.js",
  ]),
]);

export default eslintConfig;
