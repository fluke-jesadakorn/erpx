import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict owner transport for the native Support `command-plan` read route.
 *
 * The axum owner plans the exact statement for a registered read kind; this
 * bridge emits no SQL, accepts no SQL from a caller, and executes the returned
 * statement on the caller's already-open unit so the read observes uncommitted
 * effects in the caller's serializable transaction. Read kinds carry no
 * receipt/audit/outbox ledger. Missing configuration or any configured failure
 * fails closed with no TypeScript fallback.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string; companyId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

/** Read-kind ids shared with the Rust read registry in `native_support_read_sql.rs`. */
export const SUPPORT_RD_SERVICE_COVERAGE_HISTORY = 'support_rd_service_coverage_history';
export const SUPPORT_RD_SERVICE_TEAM_COVERAGE = 'support_rd_service_team_coverage';
export const SUPPORT_RD_SERVICE_TEAM_NAME = 'support_rd_service_team_name';
export const SUPPORT_RD_TICKET_PORTAL_RESPONSE = 'support_rd_ticket_portal_response';
export const SUPPORT_RD_TICKET_PORTAL_ACKNOWLEDGED_AT = 'support_rd_ticket_portal_acknowledged_at';
export const SUPPORT_RD_TICKET_EVENT_HISTORY = 'support_rd_ticket_event_history';
export const SUPPORT_RD_INCIDENT_EVENT_HISTORY = 'support_rd_incident_event_history';
export const SUPPORT_RD_TICKET_INCIDENT_LINKS = 'support_rd_ticket_incident_links';
export const SUPPORT_RD_INCIDENT_TICKET_LINKS = 'support_rd_incident_ticket_links';
export const SUPPORT_RD_INCIDENT_TICKET_CANDIDATES = 'support_rd_incident_ticket_candidates';
export const SUPPORT_RD_TICKET_SOURCE_IMAGE = 'support_rd_ticket_source_image';
export const SUPPORT_RD_TICKET_ATTACHMENT_SOURCE_IMAGE = 'support_rd_ticket_attachment_source_image';
export const SUPPORT_RD_ATTACHMENT_MIGRATION_SCOPE = 'support_rd_attachment_migration_scope';
export const SUPPORT_RD_SERVICE_TEAM_OPTIONS = 'support_rd_service_team_options';
export const SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY = 'support_rd_service_team_option_history';
export const SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS = 'support_rd_service_team_option_handlers';
export const SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE = 'support_rd_service_team_option_coverage';

const unavailable = (): never => {
  throw new ApiError(503, 'SUPPORT_NATIVE_READ_UNAVAILABLE', 'The native Support read service is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const identifier = (v: unknown): v is string => typeof v === 'string' && v.length > 0 && new TextEncoder().encode(v).length <= 200;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
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
        typeof item.holdingId !== 'string' || typeof item.companyId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password ||
        !identifier(item.companyId) || !/^[a-z0-9_-]+$/.test(item.holdingSlug)) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId, companyId: item.companyId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Plan an owner read in Rust and run it on the caller's open unit. Pure read:
 * the kind selects the SQL; only routing-scoped values cross the wire. */
export async function nativeSupportReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<SqlResult<T>> {
  const config = configuration();
  const holding = config.holdings[0]!;
  const key = 'sup-read-' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(values)).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/support/command-plan`, config.privateBaseUrl);
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
        'idempotency-key': key,
        'x-erpx-context-company': holding.companyId,
      },
      body: JSON.stringify({ kind, values, now: Date.now() }),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    const payload = await response.json().catch((): unknown => null);
    const err = record(payload) ? (payload as { error?: { code?: string; message?: string } }).error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(response.status, err?.code ?? 'SUPPORT_NATIVE_READ_FAILED', err?.message ?? 'The native Support read was rejected.');
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const statements = record(payload) ? (payload as { data?: { statements?: unknown } }).data?.statements : undefined;
  if (!Array.isArray(statements) || statements.length !== 1) return unavailable();
  const planned = statements.map((statement: unknown) => {
    if (!record(statement) || typeof statement.sql !== 'string' || !Array.isArray(statement.values)) return unavailable();
    return db.prepare(statement.sql).bind(...statement.values);
  });
  const results = await db.batch<T>(planned);
  return results[0] ?? { results: [], success: true, meta: { changes: 0, duration: 0 } };
}
