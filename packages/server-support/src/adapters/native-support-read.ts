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
export const SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE = 'support_rd_ticket_portal_ticket_image';
export const SUPPORT_RD_TICKET_PORTAL_INSTALLED = 'support_rd_ticket_portal_installed';
export const SUPPORT_RD_TICKET_EVENT_HISTORY = 'support_rd_ticket_event_history';
export const SUPPORT_RD_INCIDENT_EVENT_HISTORY = 'support_rd_incident_event_history';
export const SUPPORT_RD_TICKET_INCIDENT_LINKS = 'support_rd_ticket_incident_links';
export const SUPPORT_RD_INCIDENT_TICKET_LINKS = 'support_rd_incident_ticket_links';
export const SUPPORT_RD_INCIDENT_TICKET_CANDIDATES = 'support_rd_incident_ticket_candidates';
export const SUPPORT_RD_SLA_WORKSPACE_TICKETS = 'support_rd_sla_workspace_tickets';
export const SUPPORT_RD_SLA_POLICY_POINT = 'support_rd_sla_policy_point';
export const SUPPORT_RD_SLA_POLICY_APPROVED = 'support_rd_sla_policy_approved';
export const SUPPORT_RD_SLA_POLICY_WORKSPACE = 'support_rd_sla_policy_workspace';
export const SUPPORT_RD_SLA_POLICY_WORK = 'support_rd_sla_policy_work';
export const SUPPORT_RD_PORTAL_BINDINGS_INSTALLED = 'support_rd_portal_bindings_installed';
export const SUPPORT_RD_SUPPORT_PACKAGE_PORTAL_BINDINGS = 'support_rd_support_package_portal_bindings';
export const SUPPORT_RD_SUPPORT_PACKAGE_OBLIGATION_FACTS = 'support_rd_support_package_obligation_facts';
export const SUPPORT_RD_TICKET_SOURCE_IMAGE = 'support_rd_ticket_source_image';
export const SUPPORT_RD_TICKET_ATTACHMENT_SOURCE_IMAGE = 'support_rd_ticket_attachment_source_image';
export const SUPPORT_RD_ATTACHMENT_MIGRATION_SCOPE = 'support_rd_attachment_migration_scope';
export const SUPPORT_RD_SERVICE_TEAM_OPTIONS = 'support_rd_service_team_options';
export const SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY = 'support_rd_service_team_option_history';
export const SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS = 'support_rd_service_team_option_handlers';
export const SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE = 'support_rd_service_team_option_coverage';
export const SUPPORT_RD_CALENDAR_SUPPORT_TICKETS = 'support_rd_calendar_support_tickets';
export const SUPPORT_RD_CALENDAR_SUPPORT_INCIDENTS = 'support_rd_calendar_support_incidents';
export const SUPPORT_RD_EXEC_TICKET_METRIC = 'support_rd_exec_ticket_metric';
export const SUPPORT_RD_EXEC_INCIDENT_METRIC = 'support_rd_exec_incident_metric';
export const SUPPORT_RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE = 'support_rd_incident_attachment_source_image';
export const SUPPORT_RD_INCIDENT_ATTACHMENT_SOURCE_LINKS = 'support_rd_incident_attachment_source_links';
export const SUPPORT_RD_SALES_SUPPORT_COUNTS = 'support_rd_sales_support_counts';
export const SUPPORT_RD_SALES_OPEN_SUPPORT_CASES = 'support_rd_sales_open_support_cases';
export const SUPPORT_RD_SALES_CUSTOMER_SOURCE_CASES = 'support_rd_sales_customer_source_cases';
export const SUPPORT_RD_EVIDENCE_ITEMS = 'support_rd_evidence_items';
export const SUPPORT_RD_SLA_ALERTS = 'support_rd_sla_alerts';
export const SUPPORT_RD_TICKET_RECIPIENTS = 'support_rd_ticket_recipients';
export const SUPPORT_RD_CUSTOMER_CONTACT_NOTES = 'support_rd_customer_contact_notes';
export const SUPPORT_RD_SERVICE_TEAM_MEMBERS_NAMED = 'support_rd_service_team_members_named';
export const SUPPORT_RD_WORK_TEAM_COVERAGE = 'support_rd_work_team_coverage';
export const SUPPORT_RD_SERVICE_SCOPE_COMPANY = 'support_rd_service_scope_company';
export const SUPPORT_RD_SERVICE_SCOPE_BRANCH = 'support_rd_service_scope_branch';
export const SUPPORT_RD_SERVICE_SCOPE_DEPARTMENT = 'support_rd_service_scope_department';
export const SUPPORT_RD_SERVICE_TEAM_OPTION_NODES = 'support_rd_service_team_option_nodes';
export const SUPPORT_RD_TICKET_BY_ID = 'support_rd_ticket_by_id';
export const SUPPORT_RD_INCIDENT_BY_ID = 'support_rd_incident_by_id';

/** Fixed arity of every registered kind, mirrored from the Rust read registry.
 * The command-plan route re-validates it; rejecting a mismatch here avoids an
 * unneeded round trip and keeps no SQL fallback on the caller. */
const BIND_COUNTS: Readonly<Record<string, number>> = {
  [SUPPORT_RD_SERVICE_COVERAGE_HISTORY]: 1,
  [SUPPORT_RD_SERVICE_TEAM_COVERAGE]: 5,
  [SUPPORT_RD_SERVICE_TEAM_NAME]: 2,
  [SUPPORT_RD_TICKET_PORTAL_RESPONSE]: 1,
  [SUPPORT_RD_TICKET_PORTAL_ACKNOWLEDGED_AT]: 2,
  [SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE]: 1,
  [SUPPORT_RD_TICKET_PORTAL_INSTALLED]: 0,
  [SUPPORT_RD_TICKET_EVENT_HISTORY]: 1,
  [SUPPORT_RD_INCIDENT_EVENT_HISTORY]: 1,
  [SUPPORT_RD_TICKET_INCIDENT_LINKS]: 1,
  [SUPPORT_RD_INCIDENT_TICKET_LINKS]: 1,
  [SUPPORT_RD_INCIDENT_TICKET_CANDIDATES]: 4,
  [SUPPORT_RD_SLA_WORKSPACE_TICKETS]: 5,
  [SUPPORT_RD_SLA_POLICY_POINT]: 2,
  [SUPPORT_RD_SLA_POLICY_APPROVED]: 1,
  [SUPPORT_RD_SLA_POLICY_WORKSPACE]: 2,
  [SUPPORT_RD_SLA_POLICY_WORK]: 2,
  [SUPPORT_RD_PORTAL_BINDINGS_INSTALLED]: 0,
  [SUPPORT_RD_SUPPORT_PACKAGE_PORTAL_BINDINGS]: 2,
  [SUPPORT_RD_SUPPORT_PACKAGE_OBLIGATION_FACTS]: 6,
  [SUPPORT_RD_TICKET_SOURCE_IMAGE]: 2,
  [SUPPORT_RD_TICKET_ATTACHMENT_SOURCE_IMAGE]: 2,
  [SUPPORT_RD_ATTACHMENT_MIGRATION_SCOPE]: 1,
  [SUPPORT_RD_SERVICE_TEAM_OPTIONS]: 1,
  [SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY]: 2,
  [SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS]: 2,
  [SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE]: 2,
  [SUPPORT_RD_CALENDAR_SUPPORT_TICKETS]: 20,
  [SUPPORT_RD_CALENDAR_SUPPORT_INCIDENTS]: 19,
  [SUPPORT_RD_EXEC_TICKET_METRIC]: 16,
  [SUPPORT_RD_EXEC_INCIDENT_METRIC]: 12,
  [SUPPORT_RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE]: 3,
  [SUPPORT_RD_INCIDENT_ATTACHMENT_SOURCE_LINKS]: 2,
  [SUPPORT_RD_SALES_SUPPORT_COUNTS]: 14,
  [SUPPORT_RD_SALES_OPEN_SUPPORT_CASES]: 14,
  [SUPPORT_RD_SALES_CUSTOMER_SOURCE_CASES]: 12,
  [SUPPORT_RD_EVIDENCE_ITEMS]: 1,
  [SUPPORT_RD_SLA_ALERTS]: 6,
  [SUPPORT_RD_TICKET_RECIPIENTS]: 6,
  [SUPPORT_RD_CUSTOMER_CONTACT_NOTES]: 2,
  [SUPPORT_RD_SERVICE_TEAM_MEMBERS_NAMED]: 2,
  [SUPPORT_RD_WORK_TEAM_COVERAGE]: 8,
  [SUPPORT_RD_SERVICE_SCOPE_COMPANY]: 2,
  [SUPPORT_RD_SERVICE_SCOPE_BRANCH]: 2,
  [SUPPORT_RD_SERVICE_SCOPE_DEPARTMENT]: 3,
  [SUPPORT_RD_SERVICE_TEAM_OPTION_NODES]: 6,
  [SUPPORT_RD_TICKET_BY_ID]: 9,
  [SUPPORT_RD_INCIDENT_BY_ID]: 10,
};

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

type PlannedSupportRead = { sql: string; values: readonly unknown[] };

async function planAndRun<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<{ plan: PlannedSupportRead; result: SqlResult<T> }> {
  const binds = BIND_COUNTS[kind];
  if (binds === undefined || values.length !== binds) {
    throw new ApiError(422, 'SUPPORT_NATIVE_READ_BINDS_INVALID', 'The native Support read bind set does not match its registered kind.');
  }
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
  const statement = statements[0];
  if (!record(statement) || typeof statement.sql !== 'string' || !Array.isArray(statement.values)) return unavailable();
  const planned = [db.prepare(statement.sql).bind(...statement.values)];
  const results = await db.batch<T>(planned);
  return {
    plan: { sql: statement.sql, values: statement.values },
    result: results[0] ?? { results: [], success: true, meta: { changes: 0, duration: 0 } },
  };
}

/** Plan an owner read in Rust and run it on the caller's open unit. Pure read:
 * the kind selects the SQL; only routing-scoped values cross the wire. */
export async function nativeSupportReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<SqlResult<T>> {
  return (await planAndRun<T>(kind, values, db)).result;
}

/** Single-row form of the strict native read. The kind selects the SQL; only
 * routing-scoped values cross the wire and no SQL fallback remains. */
export async function nativeSupportReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  return (await planAndRun<T>(kind, values, db)).result.results[0] ?? null;
}

/** Same strict native plan, but also returns the owner-planned SQL for the
 * bounded executive metric source drill-down, which derives its source query
 * from the exact statement. The SQL still originates only in the native
 * registry; no SQL is accepted from the caller. */
export async function nativeSupportReadPlan<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<{ sql: string; values: readonly unknown[]; results: T[] }> {
  const { plan, result } = await planAndRun<T>(kind, values, db);
  return { sql: plan.sql, values: plan.values, results: result.results };
}
