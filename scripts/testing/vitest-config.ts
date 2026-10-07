import { fileURLToPath } from "node:url";
import type { ViteUserConfig as UserConfig } from "vitest/config";
import { loadCatalogue } from "./catalogue";

export function vitestConfiguration(kind: "unit" | "integration" | "all"): UserConfig {
  const lanes = kind === "all" ? ["unit", "integration"] : [kind];
  const suites = loadCatalogue().suites.filter(suite => lanes.includes(suite.lane));
  const selected = process.env.ERPX_SELECTED_TEST_FILES ? JSON.parse(process.env.ERPX_SELECTED_TEST_FILES) as string[] : suites.flatMap(suite => suite.files);
  const allowed = suites.flatMap(suite => suite.files).concat(process.env.ERPX_SELECTED_PROVIDER === "1" ? loadCatalogue().suites.filter(suite => suite.lane === "provider").flatMap(suite => suite.files) : []);
  if (!selected.length || selected.some(file => !allowed.includes(file))) throw new Error("INVALID_VITEST_FILE_SELECTION");
  return {
    resolve: { alias: { "@": fileURLToPath(new URL("../../src", import.meta.url)), "server-only": fileURLToPath(new URL("../../tests/server-only.ts", import.meta.url)) } },
    test: { include: selected, setupFiles: kind === "unit" ? [] : ["./tests/runtime-setup.ts"], fileParallelism: false, hookTimeout: 60_000, testTimeout: 30_000, allowOnly: !process.env.CI, retry: 0,
      reporters: ["default", fileURLToPath(new URL("./required-cases-reporter.ts", import.meta.url))] },
  };
}
