import { randomUUID } from "node:crypto";
import { AsyncLocalStorage } from "node:async_hooks";
import { measureReads } from "./read-metrics";
import { runWithRequestMemo } from "../modules/runtime/public/runtime/request-memo";

const shared = globalThis as typeof globalThis & { erpxReadRequest?: AsyncLocalStorage<string> };
const requestIds = shared.erpxReadRequest ??= new AsyncLocalStorage<string>();
export function currentReadRequestId() { return requestIds.getStore(); }

/** Explicitly owned local release cohort only; installed services cannot enable it accidentally. */
export function ownedReleaseReadDiagnostics(env:Record<string,string|undefined>) {
  const run=env.ERPX_BROWSER_SIMULATION_RUN_ID;
  if(!run||!/^([a-f0-9]{12})$/.test(run))return false;
  const directory=env.ERPX_DATA_DIR;
  return env.NODE_ENV==='production'&&env.ERPX_PMS_RELEASE_DIAGNOSTICS==='1'&&env.ERPX_PERF_LOG==='1'&&
    env.ERPX_ENVIRONMENT==='local'&&env.ERPX_BROWSER_SIMULATION==='thai-hotel-v1'&&
    env.ERPX_BROWSER_SIMULATION_PORT==='3104'&&env.ERPX_PUBLIC_ORIGIN==='http://127.0.0.1:3104'&&
    Boolean(directory?.endsWith(`/.runtime/browser/${run}`))&&env.ERPX_DATABASE_MANIFEST===`${directory}/databases.json`;
}
/** Development or guarded release diagnostics. Generated IDs contain no actor/SQL data. */
export function measureReadRequest(name: string, load: () => Promise<Response>): Promise<Response> {
  return runWithRequestMemo(() => measureRequest(name,load));
}
function measureRequest(name: string, load: () => Promise<Response>): Promise<Response> {
  if ((process.env.NODE_ENV !== "development" || process.env.ERPX_PERF_LOG !== "1") && !ownedReleaseReadDiagnostics(process.env)) return load();
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
    if(measured.metrics.lockQueries) measured.result.headers.append("Server-Timing", `lock_sql;dur=${(measured.metrics.lockQueryMs??0).toFixed(2)}, lock_queries;desc="${measured.metrics.lockQueries}"`);
    if(measured.metrics.serializationFailures || measured.metrics.deadlockFailures) measured.result.headers.append("Server-Timing", `serialization_failures;desc="${measured.metrics.serializationFailures??0}", deadlock_failures;desc="${measured.metrics.deadlockFailures??0}", transaction_retries;desc="${measured.metrics.transactionRetries??0}"`);
    return measured.result;
  });
}
