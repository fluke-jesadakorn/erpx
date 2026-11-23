import 'server-only';
import { randomUUID } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict owner transport for the mounted native Workflow `wave3-command`
 * planner. The owner Rust registry resolves a fixed read `kind` to one exact
 * read statement; this bridge emits no SQL, accepts no SQL from a caller and executes
 * the returned statement on the caller's already-open unit so the read observes
 * uncommitted effects in the caller's serializable transaction. Read kinds
 * carry no receipt/audit/outbox ledger. Missing configuration or any failed
 * configured request fails closed with no TypeScript fallback.
 */
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number }>;
type Planned = { sql: string; values: unknown[] };

const unavailable = (): never => {
  throw new ApiError(503, 'NATIVE_WORKFLOW_READ_UNAVAILABLE', 'The native Workflow reader is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_WORKFLOW_WAVE3_CONFIG;
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string') return unavailable();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    value.privateBaseUrl !== base.origin
  ) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs };
}

/** Ask the owner to plan one read; only the typed kind, its positional values
 * and the Company scope cross the wire. The reply is validated to carry one
 * owner-authored statement with an exact bind list. */
async function plan(kind: string, values: readonly unknown[], companyId: string): Promise<Planned> {
  if (!/^[a-z0-9_]{3,80}$/.test(kind) || values.length < 1 || values.length > 64 ||
      !companyId || companyId.length > 200) return unavailable();
  const config = configuration();
  const url = new URL(`/api/v1/companies/${encodeURIComponent(companyId)}/workflow/wave3-command`, config.privateBaseUrl);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        accept: 'application/json',
        'cache-control': 'no-cache',
        'idempotency-key': randomUUID(),
        'x-erpx-context-company': companyId,
      },
      body: JSON.stringify({ kind, values, now: Date.now() }),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const data = record(payload) && record(payload.data) ? payload.data : null;
  if (!data || typeof data.sql !== 'string' || !Array.isArray(data.values) || data.values.length > 64) return unavailable();
  const planned = data.sql as string;
  if (!/^\s*select\b/i.test(planned) || /;\s*\S/.test(planned)) return unavailable();
  if (planned.split('?').length - 1 !== data.values.length) return unavailable();
  return { sql: planned, values: data.values };
}

/** Planned owner read returning all rows, executed on the caller's unit. */
export async function nativeWorkflowReadAll<T>(
  kind: string,
  values: readonly unknown[],
  companyId: string,
  db: SqlDatabase,
): Promise<T[]> {
  const statement = await plan(kind, values, companyId);
  return (await db.prepare(statement.sql).bind(...statement.values).all<T>()).results;
}

/** Planned owner read returning a single row (retired `.first()` parity). */
export async function nativeWorkflowReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  companyId: string,
  db: SqlDatabase,
): Promise<T | null> {
  const statement = await plan(kind, values, companyId);
  return db.prepare(statement.sql).bind(...statement.values).first<T>();
}
