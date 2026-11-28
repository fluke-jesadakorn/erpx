import 'server-only';
import { randomUUID } from 'node:crypto';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

/**
 * Strict owner transport for the mounted native Talent `command-plan` read
 * route. The axum owner plans the exact statement for a registered read kind;
 * this bridge emits no SQL, accepts no SQL from a caller and executes the
 * returned statement on the caller's already-open unit so the read observes
 * uncommitted effects in the caller's serializable transaction. Read kinds
 * carry no receipt/audit/outbox ledger. Missing configuration or any failed
 * configured request fails closed with no TypeScript fallback.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

const unavailable = (): never => {
  throw new ApiError(503, 'NATIVE_TALENT_READ_UNAVAILABLE', 'The native Talent read service is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    value.privateBaseUrl !== base.origin
  ) return unavailable();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' ||
        typeof item.holdingId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password ||
        !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug)) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

type Planned = { sql: string; values: unknown[] };

/** Ask the owner to plan one read; only the typed kind and positional values
 * cross the wire. The reply is validated to carry exactly one owner statement. */
async function plan(kind: string, values: readonly unknown[]): Promise<Planned> {
  const config = configuration();
  const holding = config.holdings[0]!;
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/talent/command-plan`, config.privateBaseUrl);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: {
        host: new URL(holding.origin).host,
        origin: holding.origin,
        'content-type': 'application/json',
        accept: 'application/json',
        'cache-control': 'no-cache',
        'idempotency-key': randomUUID(),
        'x-erpx-context-company': holding.holdingId,
      },
      body: JSON.stringify({ kind, values }),
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
  const statements = record(payload) ? (payload as { data?: { statements?: unknown } }).data?.statements : undefined;
  if (!Array.isArray(statements) || statements.length !== 1) return unavailable();
  const entry = statements[0];
  if (!record(entry) || typeof entry.sql !== 'string' || !Array.isArray(entry.values) || entry.values.length > 64) return unavailable();
  return { sql: entry.sql, values: entry.values };
}

/** Owner-planned statement for a registered read kind. Exposed only so a
 * caller's deferred witness guard can reuse the exact owner statement without
 * authoring any SQL locally; the guard still executes on the caller's unit. */
export async function nativeTalentPlan(kind: string, values: readonly unknown[]): Promise<Planned> {
  return plan(kind, values);
}

/** Planned owner read returning all rows, executed on the caller's unit. */
export async function nativeTalentReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T[]> {
  const statement = await plan(kind, values);
  return (await db.prepare(statement.sql).bind(...statement.values).all<T>()).results;
}

/** Planned owner read returning a single row (retired `.first()` parity). */
export async function nativeTalentReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const statement = await plan(kind, values);
  return db.prepare(statement.sql).bind(...statement.values).first<T>();
}
