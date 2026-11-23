import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import { assetPurchaseControls, withAssetPurchasedRead } from '@erpx/server-assets/public/purchases';
import type { WorkItem } from '@erpx/feature-work_projection/lib/work-item';

/**
 * Native preventive-plan read transport. The mounted owner route
 * `/api/v1/holdings/<holding>/maintenance/plans` (list and `?id=` detail) is the
 * sole read authority. `ERPX_NATIVE_MAINTENANCE_CONFIG` selects the private
 * service; when it is absent the read is disabled rather than falling back to
 * TypeScript SQL, and a configured read that fails surfaces a closed error.
 */
type Row = Record<string, unknown>;
type Settings = {
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: { origin: string; holdingSlug: string; holdingId: string }[];
};
export type NativePreventivePlan = {
  id: string; company_id: string; branch_id: string; department_id: string | null;
  asset_id: string; title: string; priority: string; owner_id: string;
  interval_days: number; time_zone: string; first_due_local: string;
  next_due_local: string; next_due_at: number; lead_days: number; status: string;
  version: number; created_by: string; edited_by: string; approved_by: string | null;
  approved_at: number | null; created_at: number; updated_at: number;
  asset_code: string; asset_name: string; owner_name: string | null; owner_eligible: number;
};
export type NativePreventivePlanDetail = {
  plan: NativePreventivePlan;
  actorId: string;
  canWrite: boolean;
  canReview: boolean;
  assetVersion: number | null;
  people: { id: string; name: string }[];
  history: Row[];
  occurrences: Row[];
  purchases?: Record<string, { newWork: boolean; settleExisting: boolean; history: boolean; revision: string }>;
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
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);
const optionalInteger = (value: unknown) => value === null || integer(value, Number.MIN_SAFE_INTEGER);
const priorities = new Set(['normal', 'high', 'critical']);
const planStatuses = new Set(['draft', 'active', 'paused', 'cancelled']);
const planKeys = [
  'id', 'company_id', 'branch_id', 'department_id', 'asset_id', 'title', 'priority', 'owner_id',
  'interval_days', 'time_zone', 'first_due_local', 'next_due_local', 'next_due_at', 'lead_days',
  'status', 'version', 'created_by', 'edited_by', 'approved_by', 'approved_at', 'created_at',
  'updated_at', 'asset_code', 'asset_name', 'owner_name', 'owner_eligible',
] as const;

const planRow = (row: unknown): row is NativePreventivePlan =>
  exact(row, planKeys) &&
  identifier(row.id) && identifier(row.company_id) && identifier(row.branch_id) &&
  optionalText(row.department_id, 200) && identifier(row.asset_id) &&
  text(row.title, 160) && priorities.has(String(row.priority)) && identifier(row.owner_id) &&
  integer(row.interval_days, 1) && text(row.time_zone, 80) &&
  text(row.first_due_local, 64) && text(row.next_due_local, 64) &&
  integer(row.next_due_at, Number.MIN_SAFE_INTEGER) && integer(row.lead_days, 0) &&
  planStatuses.has(String(row.status)) && integer(row.version, 1) &&
  identifier(row.created_by) && identifier(row.edited_by) &&
  optionalText(row.approved_by, 200) && optionalInteger(row.approved_at) &&
  integer(row.created_at, Number.MIN_SAFE_INTEGER) && integer(row.updated_at, Number.MIN_SAFE_INTEGER) &&
  text(row.asset_code, 200) && text(row.asset_name, 400) &&
  optionalText(row.owner_name, 400) && [0, 1].includes(Number(row.owner_eligible));

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
export function nativePreventiveReadConfigured(): boolean {
  return process.env.ERPX_NATIVE_MAINTENANCE_CONFIG !== undefined;
}

async function read(actor: WorkContext, id?: string): Promise<Row | undefined> {
  const raw = process.env.ERPX_NATIVE_MAINTENANCE_CONFIG;
  if (raw === undefined) return undefined;
  const config = settings(raw);
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
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/maintenance/plans`, config.privateBaseUrl);
  if (id !== undefined) url.searchParams.set('id', id);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, { method: 'GET', headers: h, signal: controller.signal, redirect: 'error', cache: 'no-store' });
  } catch { return fail(503, 'NATIVE_MAINTENANCE_READ_UNAVAILABLE'); }
  finally { clearTimeout(timer); }
  if (response.status === 403 || response.status === 404) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status, 'MAINTENANCE_PLAN_NOT_FOUND', 'Preventive plan not found in this scope.');
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
  return parsed;
}

async function readPlans(actor: WorkContext): Promise<NativePreventivePlan[] | undefined> {
  const parsed = await read(actor);
  if (parsed === undefined) return undefined;
  if (!Array.isArray(parsed.data) || parsed.data.length > 300 || !parsed.data.every(row => record(row) && planRow(row)))
    return fail(503, 'NATIVE_MAINTENANCE_RESPONSE_INVALID');
  return parsed.data as NativePreventivePlan[];
}

async function readPlanDetail(actor: WorkContext, id: string): Promise<NativePreventivePlanDetail | undefined> {
  const parsed = await read(actor, id);
  if (parsed === undefined) return undefined;
  const data = parsed.data;
  if (!record(data) || !planRow(data.plan) || data.plan.id !== id ||
    !identifier(data.actorId) || typeof data.canWrite !== 'boolean' || typeof data.canReview !== 'boolean' ||
    (data.assetVersion !== null && !integer(data.assetVersion, 1)) ||
    !Array.isArray(data.people) || !data.people.every(person => exact(person, ['id', 'name']) && identifier(person.id) && text(person.name, 400)) ||
    !Array.isArray(data.history) || data.history.length > 500 || !data.history.every(record) ||
    !Array.isArray(data.occurrences) || data.occurrences.length > 500 || !data.occurrences.every(record))
    return fail(503, 'NATIVE_MAINTENANCE_RESPONSE_INVALID');
  return data as unknown as NativePreventivePlanDetail;
}

function planWork(actor: WorkContext, plans: NativePreventivePlan[]): WorkItem[] {
  const now = Date.now(), review = hasPermission(actor, 'records.review'), write = hasPermission(actor, 'records.write');
  return plans.map(plan => {
    const terminal = plan.status === 'cancelled';
    const own = plan.owner_id === actor.userId && Boolean(Number(plan.owner_eligible));
    const due = plan.next_due_at <= now + plan.lead_days * 86_400_000;
    const canAct = write && !terminal && (
      plan.status === 'draft' || plan.status === 'paused'
        ? review && plan.edited_by !== actor.userId
        : plan.status === 'active' && due && (own || review)
    );
    const href = `/maintenance/plans/${plan.id}`;
    return {
      source: { kind: 'maintenance_plan', id: plan.id, version: plan.version, href, label: `Preventive maintenance · ${plan.title}` },
      id: plan.id, type: 'maintenance_plan', title: `${plan.asset_code} · ${plan.title}`,
      detail: `Preventive · every ${plan.interval_days} days · ${plan.time_zone}`,
      href, status: plan.status, updatedAt: plan.updated_at, dueAt: plan.next_due_at,
      queue: terminal ? 'completed' : canAct ? 'todo' : 'waiting', canAct,
      responsibleQueue: 'Preventive maintenance owner',
      nextAction: plan.status === 'active' ? 'Generate due work' : plan.status === 'cancelled' ? 'No further action' : 'Review and activate preventive plan',
      blocker: Number(plan.owner_eligible) ? null : 'Assigned maintenance owner is no longer eligible; pause and reassign the plan',
      relationships: [plan.owner_id === actor.userId ? 'assigned' : plan.created_by === actor.userId ? 'requested' : 'team'],
    };
  });
}

const nativeUnavailable = (): never => fail(503, 'NATIVE_MAINTENANCE_UNAVAILABLE', 'The native Maintenance read is unavailable.');

/** Native plan list. No owner SQL is built in TypeScript. */
export async function preventivePlans(actor: WorkContext, providedWrite?: SqlDatabase): Promise<NativePreventivePlan[]> {
  return withAssetPurchasedRead(actor, 'assets.preventive', async () => {
    if (!hasPermission(actor, 'records.read') || !hasModuleAccess(actor.modules, 'maintenance')) return [];
    const plans = await readPlans(actor);
    if (plans === undefined) {
      if (process.env.NODE_ENV === 'production') return nativeUnavailable();
      return [];
    }
    return plans;
  }, providedWrite);
}

/** Native plan detail, used for existence/visibility checks by notification consumers. */
export async function preventivePlanDetail(actor: WorkContext, id: string): Promise<NativePreventivePlanDetail> {
  return withAssetPurchasedRead(actor, 'assets.preventive', async () => {
    const detail = await readPlanDetail(actor, id);
    if (detail === undefined) return nativeUnavailable();
    return { ...detail, purchases: await assetPurchaseControls(actor) };
  });
}

/** Native plan-derived Work projection. Production fails closed without config. */
export async function preventivePlanWork(actor: WorkContext): Promise<WorkItem[]> {
  if (!hasPermission(actor, 'records.read') || !hasModuleAccess(actor.modules, 'maintenance')) return [];
  const purchase = (await assetPurchaseControls(actor, ['assets.preventive']))?.['assets.preventive'];
  if (purchase && !purchase.history) return [];
  let plans: NativePreventivePlan[] | undefined;
  try {
    plans = await withAssetPurchasedRead(actor, 'assets.preventive', () => readPlans(actor));
  } catch (error) {
    if (error instanceof ApiError && error.code === 'NATIVE_MAINTENANCE_READ_UNAVAILABLE' && process.env.NODE_ENV !== 'production') return [];
    throw error;
  }
  if (plans === undefined) {
    if (process.env.NODE_ENV === 'production') return nativeUnavailable();
    return [];
  }
  const items = planWork(actor, plans);
  return purchase && !purchase.newWork ? items.map(item => ({ ...item, canAct: false, queue: 'waiting' as const })) : items;
}
