import { defineConfig } from "vitest/config";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../../", import.meta.url));
export default defineConfig({
  root,
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("../../../src", import.meta.url)),
      "server-only": fileURLToPath(new URL("../../support/server-only.ts", import.meta.url)),
    },
  },
  test: {
    include: ["tests/unit/planning/*.test.ts"],
    pool: "forks",
    isolate: true,
    testTimeout: 30_000,
  },
});
