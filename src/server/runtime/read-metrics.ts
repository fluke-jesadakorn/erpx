import { AsyncLocalStorage } from "node:async_hooks";

export type ReadMetrics = { queries: number; queryMs: number; poolWaitMs: number };
const shared = globalThis as typeof globalThis & { erpxReadMetrics?: AsyncLocalStorage<ReadMetrics> };
const metrics = shared.erpxReadMetrics ??= new AsyncLocalStorage<ReadMetrics>();
export function currentReadMetrics() { return metrics.getStore(); }
export async function measureReads<T>(load: () => Promise<T>) {
  const values: ReadMetrics = { queries: 0, queryMs: 0, poolWaitMs: 0 };
  const started = performance.now();
  const result = await metrics.run(values, load);
  return { result, metrics: { ...values, durationMs: performance.now() - started } };
}

/** Timing labels contain no actor identifiers, credentials or SQL parameters. */
export async function measureReadPhase<T>(phase: string, load: () => Promise<T>): Promise<T> {
  if (process.env.ERPX_PERF_LOG !== "1") return load();
  const measured = await measureReads(load);
  console.info(JSON.stringify({event:"page_read",phase,...measured.metrics}));
  return measured.result;
}
