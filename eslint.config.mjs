import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    "**/.next/**",
    "**/.build/**",
    "**/.cache/**",
    "**/node_modules/**",
    // Cargo's workspace output changes while Rust checks run in parallel.
    "target/**",
    "apps/*/next-env.d.ts",
    ".open-next/**",
    ".wrangler/**",
    ".scratch/**",
    ".runtime/**",
    "playwright-report/**",
    "test-results/**",
    "public/vendor/**",
    "out/**",
    "build/**",
    "cloudflare-env.d.ts",
    "next-env.d.ts",
  ]),
]);

export default eslintConfig;
