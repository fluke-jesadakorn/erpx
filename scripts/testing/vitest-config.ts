import { fileURLToPath } from "node:url";
import { availableParallelism } from "node:os";
import type { ViteUserConfig as UserConfig } from "vitest/config";
import { loadCatalogue } from "./catalogue";

/** Child CLI validators and artifact proofs compete for the same CPU budget.
 * Cap unit forks to the CPUs available to this process, including hosted runners. */
export function unitWorkerLimit(parallelism: number): number {
  if (!Number.isSafeInteger(parallelism) || parallelism < 1) throw new Error("POSITIVE_AVAILABLE_PARALLELISM_REQUIRED");
  return Math.min(8, parallelism);
}

export function vitestConfiguration(kind: "unit" | "integration" | "all"): UserConfig {
  const lanes = kind === "all" ? ["unit", "integration"] : [kind];
  const suites = loadCatalogue().suites.filter(suite => lanes.includes(suite.lane));
  const selected = process.env.ERPX_SELECTED_TEST_FILES ? JSON.parse(process.env.ERPX_SELECTED_TEST_FILES) as string[] : suites.flatMap(suite => suite.files);
  const allowed = suites.flatMap(suite => suite.files).concat(process.env.ERPX_SELECTED_PROVIDER === "1" ? loadCatalogue().suites.filter(suite => suite.lane === "provider").flatMap(suite => suite.files) : []);
  if (!selected.length || selected.some(file => !allowed.includes(file))) throw new Error("INVALID_VITEST_FILE_SELECTION");
  const legacyBenchmark = process.env.ERPX_FIXTURE_BENCHMARK_MODE === "legacy-raw";
  if (legacyBenchmark && process.env.ERPX_PERFORMANCE_BENCHMARK !== "workspace-fixtures-v1") throw new Error("EXPLICIT_FIXTURE_BENCHMARK_REQUIRED");
  return {
    resolve: { alias: { "@": fileURLToPath(new URL("../../src", import.meta.url)), "server-only": fileURLToPath(new URL("../../tests/support/server-only.ts", import.meta.url)) } },
    test: { include: selected, setupFiles: kind === "unit" ? [] : ["./tests/support/runtime-setup.ts"],
      globalSetup: kind === "unit" ? [] : ["./tests/support/postgres-global-setup.ts"],
      pool: "forks", isolate: true, fileParallelism: !legacyBenchmark, maxWorkers: kind === "unit" ? unitWorkerLimit(availableParallelism()) : legacyBenchmark ? 1 : 2,
      hookTimeout: 60_000, testTimeout: 30_000, allowOnly: !process.env.CI, retry: 0,
      reporters: ["default", fileURLToPath(new URL("./required-cases-reporter.ts", import.meta.url))] },
  };
}
