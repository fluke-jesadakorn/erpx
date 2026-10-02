import { randomUUID } from "node:crypto";
import { AsyncLocalStorage } from "node:async_hooks";
import { measureReads } from "./read-metrics";

const shared = globalThis as typeof globalThis & { erpxReadRequest?: AsyncLocalStorage<string> };
const requestIds = shared.erpxReadRequest ??= new AsyncLocalStorage<string>();
export function currentReadRequestId() { return requestIds.getStore(); }

/** Development diagnostics only. Request IDs are generated here, not supplied by callers. */
export function measureReadRequest(name: string, load: () => Promise<Response>): Promise<Response> {
  if (process.env.NODE_ENV !== "development" || process.env.ERPX_PERF_LOG !== "1") return load();
  const requestId = randomUUID();
  return requestIds.run(requestId, async () => {
    let status: number | undefined;
    const measured = await measureReads(async () => {
      const response = await load();
      status = response.status;
      return response;
    }, values => console.info(JSON.stringify({ event: "read_request", requestId, name, status,
      ...values, failed: values.failed || (status !== undefined && status >= 400) })));
    measured.result.headers.set("X-ERPX-Read-Id", requestId);
    measured.result.headers.append("Server-Timing", `erpx;dur=${measured.metrics.durationMs.toFixed(2)}, sql;dur=${measured.metrics.queryMs.toFixed(2)}, pool;dur=${measured.metrics.poolWaitMs.toFixed(2)}, queries;desc="${measured.metrics.queries}"`);
    return measured.result;
  });
}
