import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/platform-runtime-server/public/runtime/types';

/**
 * Strict owner transport for the native Operations `command-plan` read route.
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

/** Read-kind ids shared with the Rust read registry in `native_operations_read_sql.rs`. */
export const OPERATIONS_RD_TASK_PROJECTION_SOURCE = 'operations_rd_task_projection_source';
export const OPERATIONS_RD_APP_UPLOAD_TASK_STATE = 'operations_rd_app_upload_task_state';
export const OPERATIONS_RD_FLOW_DEFINITIONS_DELEGATION = 'operations_rd_flow_definitions_delegation';
export const OPERATIONS_RD_BETA_MANAGEMENT_TASK_COMPANY = 'operations_rd_beta_management_task_company';
export const OPERATIONS_RD_BETA_MANAGEMENT_TASK_FILE_OWNER = 'operations_rd_beta_management_task_file_owner';
export const OPERATIONS_RD_ROUTINE_TEMPLATE_SCHEDULES = 'operations_rd_routine_template_schedules';
export const OPERATIONS_RD_FIELD_EXCEPTION = 'operations_rd_field_exception';
export const OPERATIONS_RD_FIELD_POLICY = 'operations_rd_field_policy';
export const OPERATIONS_RD_LEGACY_ROUTINE_TEMPLATES = 'operations_rd_legacy_routine_templates';
export const OPERATIONS_RD_OPERATIONS_QUEUE_POLICIES = 'operations_rd_operations_queue_policies';
export const OPERATIONS_RD_OPERATIONS_EXCEPTION_EVENTS = 'operations_rd_operations_exception_events';
export const OPERATIONS_RD_RECORD_TASK_PROJECT = 'operations_rd_record_task_project';
export const OPERATIONS_RD_WORK_TASK_SOURCE = 'operations_rd_work_task_source';
export const OPERATIONS_RD_TASK_ATTACHMENT_ASSIGNEE = 'operations_rd_task_attachment_assignee';
export const OPERATIONS_RD_WORK_TASK_ALERT_SOURCE = 'operations_rd_work_task_alert_source';
export const OPERATIONS_RD_WORK_TASK_IMAGE = 'operations_rd_work_task_image';
export const OPERATIONS_RD_WORK_TASK_ALERT_IMAGE = 'operations_rd_work_task_alert_image';
export const OPERATIONS_RD_HANDOVER_RECOVERY_RECEIPT = 'operations_rd_handover_recovery_receipt';
export const OPERATIONS_RD_WORKFLOW_DOCUMENT_DETAIL = 'operations_rd_workflow_document_detail';
export const OPERATIONS_RD_WORKFLOW_DOCUMENT_PAGE = 'operations_rd_workflow_document_page';
export const OPERATIONS_RD_WORK_TASK_REPLAY_RECEIPT = 'operations_rd_work_task_replay_receipt';
export const OPERATIONS_RD_ATTACHMENT_OWNER_SCOPE = 'operations_rd_attachment_owner_scope';
export const OPERATIONS_RD_TASK_DETAIL_ATTACHMENTS = 'operations_rd_task_detail_attachments';
export const OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS = 'operations_rd_task_attachment_purchase_witness';
export const OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS_OWNED = 'operations_rd_task_attachment_purchase_witness_owned';

const unavailable = (): never => {
  throw new ApiError(503, 'OPERATIONS_NATIVE_READ_UNAVAILABLE', 'The native Operations read service is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const identifier = (v: unknown): v is string => typeof v === 'string' && v.length > 0 && new TextEncoder().encode(v).length <= 200;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
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

type Planned = { sql: string; values: unknown[] };

/** Ask the owner to plan one read; only the typed kind and routing values cross
 * the wire. The reply is validated to carry exactly one owner statement. */
async function plan(kind: string, values: readonly unknown[]): Promise<Planned> {
  const config = configuration();
  const holding = config.holdings[0]!;
  const key = 'ops-read-' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(values)).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/operations/command-plan`, config.privateBaseUrl);
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
      throw new ApiError(response.status, err?.code ?? 'OPERATIONS_NATIVE_READ_FAILED', err?.message ?? 'The native Operations read was rejected.');
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const statements = record(payload) ? (payload as { data?: { statements?: unknown } }).data?.statements : undefined;
  if (!Array.isArray(statements) || statements.length !== 1) return unavailable();
  const statement = statements[0];
  if (!record(statement) || typeof statement.sql !== 'string' || !Array.isArray(statement.values)) return unavailable();
  return { sql: statement.sql, values: statement.values };
}

/** Plan an owner read in Rust and run it on the caller's open unit. Pure read:
 * the kind selects the SQL; only routing-scoped values cross the wire. */
export async function nativeOperationsReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<SqlResult<T>> {
  const planned = await plan(kind, values);
  const results = await db.batch<T>([db.prepare(planned.sql).bind(...planned.values)]);
  return results[0] ?? { results: [], success: true, meta: { changes: 0, duration: 0 } };
}

/** Return the owner-planned SQL text for a trusted server caller to compose into
 * its own statement. Used only for correlated fragments that cannot execute
 * standalone; the SQL is selected by the Rust allow-list, never by the caller. */
export async function nativeOperationsReadSql(
  kind: string,
  values: readonly unknown[] = [],
): Promise<Planned> {
  return plan(kind, values);
}

/** Run a single-row owner read and return its first row (or null). The retired
 * TypeScript `.first()` ran the same statement on the caller's unit and took
 * the first row; the native plan is executed the same way. */
export async function nativeOperationsReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const result = await nativeOperationsReadAll<T>(kind, values, db);
  return result.results[0] ?? null;
}
