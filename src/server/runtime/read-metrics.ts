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
