import "server-only";
import { AsyncLocalStorage } from "node:async_hooks";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const reads = new AsyncLocalStorage<
  { values: WeakMap<SqlDatabase, Map<string, Promise<unknown>>>; now: number }
>();
export const policyNow = () => reads.getStore()?.now ?? Date.now();
export const hasPolicyReadSnapshot = () => !!reads.getStore();
/** Reuse eligibility reads only within one read-only work-summary request. */
export function withPolicyReadSnapshot<T>(load: () => Promise<T>): Promise<T> {
  return reads.getStore() ? load() : reads.run({ values: new WeakMap(), now: Date.now() }, load);
}
export function policyRead<T>(
  db: SqlDatabase,
  key: string,
  load: () => Promise<T>,
): Promise<T> {
  const request = reads.getStore()?.values;
  if (!request) return load();
  let cache = request.get(db);
  if (!cache) {
    cache = new Map();
    request.set(db, cache);
  }
  let result = cache.get(key);
  if (!result) {
    result = load();
    if (cache.size < 2048) cache.set(key, result);
  }
  return result as Promise<T>;
}
