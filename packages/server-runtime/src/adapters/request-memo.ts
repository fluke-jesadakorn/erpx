import { AsyncLocalStorage } from 'node:async_hooks';

type RequestMemo = Map<symbol, unknown>;
const shared = globalThis as typeof globalThis & { erpxRequestMemo?: AsyncLocalStorage<RequestMemo> };
const requests = shared.erpxRequestMemo ??= new AsyncLocalStorage<RequestMemo>();

/** One HTTP invocation owns this map; no value survives into another request. */
export function runWithRequestMemo<T>(load: () => T): T {
  return requests.run(new Map(), load);
}

/** Deduplicate only explicitly selected request facts, never mutation authority. */
export function memoizeInRequest<T>(key: symbol, load: () => Promise<T>): Promise<T> {
  const memo = requests.getStore();
  if (!memo) return load();
  const existing = memo.get(key);
  if (existing) return existing as Promise<T>;
  const value = Promise.resolve().then(load);
  memo.set(key, value);
  return value;
}

/** Request-local computation state only; mutable authority is always read fresh. */
export function memoizeSyncInRequest<T>(key: symbol, load: () => T): T {
  const memo=requests.getStore();
  if(!memo)return load();
  if(memo.has(key))return memo.get(key) as T;
  const value=load(); memo.set(key,value); return value;
}
