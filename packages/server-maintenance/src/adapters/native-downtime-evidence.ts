import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import type { MaintenanceDowntimeEvidenceRow } from '@erpx/server-maintenance/domain/work';

/**
 * Native downtime-evidence read transport. The mounted owner route
 * `/api/v1/holdings/<holding>/maintenance/downtime-evidence` is the sole read
 * authority. `ERPX_NATIVE_MAINTENANCE_CONFIG` selects the private service; when
 * it is absent the read is disabled rather than falling back to TypeScript SQL,
 * and a configured read that fails surfaces a closed error.
 */
type Row = Record<string, unknown>;
type Settings = {
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: { origin: string; holdingSlug: string; holdingId: string }[];
};
export type NativeDowntimeQuery = {
  from: number; to: number; query: string; limit: number;
  cursor?: { endedAt: number; id: string }; recordId?: string;
};

const fail = (status: number, code: string, message = 'The native Maintenance read is unavailable.'): never => {
  throw new ApiError(status, code, message);
};
const record = (value: unknown): value is Row => value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const evidenceStatuses = new Set(['completed', 'cancelled']);
const evidenceKeys = [
  'id', 'version', 'status', 'title', 'asset_code', 'asset_name',
  'downtime_started_at', 'downtime_ended_at',
] as const;

const evidenceRow = (row: unknown): row is MaintenanceDowntimeEvidenceRow =>
  exact(row, evidenceKeys) &&
  identifier(row.id) && text(row.version, 40) && evidenceStatuses.has(String(row.status)) &&
  text(row.title, 400) && text(row.asset_code, 200) && text(row.asset_name, 400) &&
  text(row.downtime_started_at, 40) && text(row.downtime_ended_at, 40) &&
  /^-?\d{1,20}$/.test(String(row.downtime_started_at)) &&
  /^-?\d{1,20}$/.test(String(row.downtime_ended_at));

function settings(raw: string): Settings {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return fail(503, 'NATIVE_MAINTENANCE_CONFIG_INVALID'); }
  if (!record(value) || !text(value.privateBaseUrl) ||
    !Number.isSafeInteger(value.timeoutMs) || (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64)
    return fail(503, 'NATIVE_MAINTENANCE_CONFIG_INVALID');
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return fail(503, 'NATIVE_MAINTENANCE_CONFIG_INVALID'); }
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    base.username || base.password || value.privateBaseUrl !== base.origin)
    return fail(503, 'NATIVE_MAINTENANCE_CONFIG_INVALID');
  for (const holding of value.holdings) {
    if (!record(holding) || typeof holding.origin !== 'string' ||
      typeof holding.holdingSlug !== 'string' || !identifier(holding.holdingId))
      return fail(503, 'NATIVE_MAINTENANCE_CONFIG_INVALID');
    try {
      const origin = new URL(holding.origin);
      if (origin.protocol !== 'https:' || holding.origin !== origin.origin || origin.port)
        return fail(503, 'NATIVE_MAINTENANCE_CONFIG_INVALID');
    } catch { return fail(503, 'NATIVE_MAINTENANCE_CONFIG_INVALID'); }
  }
  return value as unknown as Settings;
}

/** Selector presence only; an absent configuration never probes the private service. */
export function nativeDowntimeReadConfigured(): boolean {
  return process.env.ERPX_NATIVE_MAINTENANCE_CONFIG !== undefined;
}

function queryParams(input: NativeDowntimeQuery): URLSearchParams {
  if (!integer(input.from, Number.MIN_SAFE_INTEGER) || !integer(input.to, Number.MIN_SAFE_INTEGER) ||
    !text(input.query, 200) || !integer(input.limit, 1) || input.limit > 50 ||
    (input.cursor !== undefined && (!record(input.cursor) ||
      !integer(input.cursor.endedAt, Number.MIN_SAFE_INTEGER) || !identifier(input.cursor.id))) ||
    (input.recordId !== undefined && !identifier(input.recordId)))
    return fail(422, 'MAINTENANCE_DOWNTIME_QUERY_INVALID', 'Use a bounded Maintenance downtime evidence query.');
  const params = new URLSearchParams();
  params.set('from', String(input.from));
  params.set('to', String(input.to));
  params.set('query', input.query);
  params.set('limit', String(input.limit));
  if (input.cursor) {
    params.set('cursorEndedAt', String(input.cursor.endedAt));
    params.set('cursorId', input.cursor.id);
  }
  if (input.recordId !== undefined) params.set('recordId', input.recordId);
  return params;
}

async function read(actor: WorkContext, input: NativeDowntimeQuery): Promise<Row[] | undefined> {
  const raw = process.env.ERPX_NATIVE_MAINTENANCE_CONFIG;
  if (raw === undefined) return undefined;
  const config = settings(raw);
  const params = queryParams(input);
  const target = config.holdings.find(h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId);
  if (!target || !actor.authority || !actor.iam) return fail(409, 'WORK_CONTEXT_CHANGED');
  const incoming = await headers();
  const host = incoming.get('host');
  if (!host || new URL(target.origin).host !== host) return fail(409, 'WORK_CONTEXT_CHANGED');
  const h = new Headers({ host, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192) return fail(503, 'NATIVE_MAINTENANCE_READ_COOKIE_LIMIT');
    h.set('cookie', cookie);
  }
  h.set('x-erpx-context-company', actor.companyId);
  h.set('x-erpx-context-assignment', actor.assignmentId);
  h.set('x-erpx-context-revision', String(actor.authority.revision));
  h.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug) h.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department') h.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/maintenance/downtime-evidence`, config.privateBaseUrl);
  url.search = params.toString();
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, { method: 'GET', headers: h, signal: controller.signal, redirect: 'error', cache: 'no-store' });
  } catch { return fail(503, 'NATIVE_MAINTENANCE_READ_UNAVAILABLE'); }
  finally { clearTimeout(timer); }
  if (response.status === 403 || response.status === 404) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status, 'MAINTENANCE_DOWNTIME_EVIDENCE_FORBIDDEN', 'Downtime evidence is not available in this scope.');
  }
  if (!response.ok || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return fail(503, 'NATIVE_MAINTENANCE_READ_UNAVAILABLE');
  }
  let parsed: unknown;
  try { parsed = await response.json(); } catch { return fail(503, 'NATIVE_MAINTENANCE_RESPONSE_INVALID'); }
  if (!record(parsed) || !record(parsed.context)) return fail(503, 'NATIVE_MAINTENANCE_RESPONSE_INVALID');
  const context = parsed.context;
  if (context.companyId !== actor.companyId || context.assignmentId !== actor.assignmentId ||
    context.revision !== actor.authority.revision || context.assignmentVersion !== actor.authority.assignmentVersion ||
    !Array.isArray(context.permissions) ||
    !actor.iam.permissions.every(permission => (context.permissions as unknown[]).includes(permission)))
    return fail(409, 'WORK_CONTEXT_CHANGED');
  if (!Array.isArray(parsed.data) || parsed.data.length > 51 || !parsed.data.every(evidenceRow))
    return fail(503, 'NATIVE_MAINTENANCE_RESPONSE_INVALID');
  return parsed.data as Row[];
}

const nativeUnavailable = (): never => fail(503, 'NATIVE_MAINTENANCE_UNAVAILABLE', 'The native Maintenance read is unavailable.');

/** Native downtime evidence. No owner SQL is built in TypeScript. */
export async function nativeDowntimeEvidenceRows(
  actor: WorkContext,
  input: NativeDowntimeQuery,
  providedWrite?: SqlDatabase,
): Promise<MaintenanceDowntimeEvidenceRow[]> {
  void providedWrite;
  if (!hasPermission(actor, 'records.read') || !hasModuleAccess(actor.modules, 'maintenance')) return [];
  const rows = await read(actor, input);
  if (rows === undefined) return nativeUnavailable();
  return rows as MaintenanceDowntimeEvidenceRow[];
}
