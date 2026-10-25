import { AsyncLocalStorage } from "node:async_hooks";

export type ReadMetrics = { queries: number; queryMs: number; poolWaitMs: number; queryErrors: number; poolErrors: number; lockQueries?:number; lockQueryMs?:number; lockQueryErrors?:number; serializationFailures?:number; deadlockFailures?:number; transactionRetries?:number };
export type ReadMeasurement = ReadMetrics & { durationMs: number; failed: boolean };
const parentScope = Symbol.for("erpx.readMetrics.parent");
type ReadScope = ReadMetrics & { [parentScope]?: ReadScope };
const shared = globalThis as typeof globalThis & { erpxReadMetrics?: AsyncLocalStorage<ReadScope | undefined>; erpxReadRequest?: AsyncLocalStorage<string> };
const metrics = shared.erpxReadMetrics ??= new AsyncLocalStorage<ReadScope | undefined>();
export function currentReadMetrics() { return metrics.getStore(); }
export function runWithReadMetrics<T>(scope: ReadScope | undefined, action: () => T): T {
  return metrics.run(scope, action);
}

/** Capture the scope at dispatch: a driver's callback can run in another scope. */
export function recordReadQuery(scope: ReadScope, durationMs: number, failed: boolean) {
  for (let target: ReadScope | undefined = scope; target; target = target[parentScope]) {
    target.queries++;
    target.queryMs += durationMs;
    target.queryErrors = (target.queryErrors ?? 0) + Number(failed);
  }
}
export function recordReadPoolWait(scope: ReadScope, durationMs: number, failed: boolean) {
  for (let target: ReadScope | undefined = scope; target; target = target[parentScope]) {
    target.poolWaitMs += durationMs;
    target.poolErrors = (target.poolErrors ?? 0) + Number(failed);
  }
}
/** Statement duration includes execution as well as any lock wait, never pure lock-wait time. */
export function recordReadLockQuery(scope:ReadScope,durationMs:number,failed:boolean) {
  for(let target:ReadScope|undefined=scope;target;target=target[parentScope]) {
    target.lockQueries=(target.lockQueries??0)+1;
    target.lockQueryMs=(target.lockQueryMs??0)+durationMs;
    target.lockQueryErrors=(target.lockQueryErrors??0)+Number(failed);
  }
}
export function recordReadTransactionRetry(error:unknown,retrying:boolean) {
  const code=(error as {code?:string}|null)?.code;
  if(code!=="40001"&&code!=="40P01")return;
  for(let target=currentReadMetrics();target;target=target[parentScope]) {
    const field=code==="40001"?"serializationFailures":"deadlockFailures";
    target[field]=(target[field]??0)+1;
    target.transactionRetries=(target.transactionRetries??0)+Number(retrying);
  }
}

export async function measureReads<T>(load: () => Promise<T>, completed?: (measurement: ReadMeasurement) => void) {
  const values: ReadScope = { queries: 0, queryMs: 0, poolWaitMs: 0, queryErrors: 0, poolErrors: 0 };
  Object.defineProperty(values, parentScope, { value: currentReadMetrics() });
  const started = performance.now();
  let failed = true;
  try {
    const result = await metrics.run(values, load);
    failed = false;
    return { result, metrics: { ...values, durationMs: performance.now() - started, failed } };
  } finally {
    completed?.({ ...values, durationMs: performance.now() - started, failed });
  }
}

/** Timing labels contain no actor identifiers, credentials or SQL parameters. */
export async function measureReadPhase<T>(phase: string, load: () => Promise<T>): Promise<T> {
  if (process.env.ERPX_PERF_LOG !== "1") return load();
  const measured = await measureReads(load, measurement => {
    console.info(JSON.stringify({event:"page_read",requestId:shared.erpxReadRequest?.getStore(),phase,...measurement}));
  });
  return measured.result;
}
