import 'server-only';
import { AsyncLocalStorage } from 'node:async_hooks';

/**
 * Current Holding for system/worker code that has no browser WorkContext.
 * Queue and job sweeps run one Holding at a time; command transports that only
 * receive a positional Company can resolve it from this scope instead of
 * guessing a Holding. Browser requests never set it.
 */
const scope = new AsyncLocalStorage<string>();

export function currentHoldingId(): string | undefined {
  return scope.getStore();
}

export function withHolding<T>(holdingId: string, run: () => T): T {
  return scope.run(holdingId, run);
}
