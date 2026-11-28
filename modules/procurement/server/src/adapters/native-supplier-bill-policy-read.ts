import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Native Procurement supplier-invoice policy read transport.
 *
 * The mounted owner route `/api/v1/holdings/<holding>/supplier-bills/policy`
 * (GET) is the sole read authority. `ERPX_NATIVE_PROCUREMENT_CONFIG` selects the
 * private service. No owner SQL is prepared or executed here: when the config
 * is absent the read is disabled and fails closed, and a configured read that
 * fails never falls back to the retained TypeScript policy query.
 */
type Row = Record<string, unknown>;
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string; companyId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

export type NativeSupplierBillPolicyEvent = {
  id: string;
  company_id: string;
  version: number;
  previous_mode: 'optional' | 'required';
  matching_mode: 'optional' | 'required';
  actor_id: string;
  reason: string;
  created_at: number;
  actor_name: string | null;
};
export type NativeSupplierBillPolicyPurchase = {
  newWork: boolean;
  settleExisting: boolean;
  history: boolean;
  revision: string;
};

/** Exact `supplierInvoicePolicy` `data` payload projected by the owner. */
export type NativeSupplierBillPolicy = {
  matchingMode: 'optional' | 'required';
  version: number;
  updatedByName: string | null;
  updatedAt: number | null;
  canManage: boolean;
  events: NativeSupplierBillPolicyEvent[];
  companyId: string;
  purchase?: NativeSupplierBillPolicyPurchase;
};

const fail = (status: number, code: string, message = 'The native Procurement policy read is unavailable.'): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (): never => fail(503, 'NATIVE_PROCUREMENT_UNAVAILABLE');
const record = (value: unknown): value is Row => value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const mode = (value: unknown): value is 'optional' | 'required' => value === 'optional' || value === 'required';
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function settings(raw: string): Settings {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  if (!integer(value.timeoutMs, 250) || (value.timeoutMs as number) > 30_000) return unavailable();
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
      !identifier(item.companyId) || !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug)) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId, companyId: item.companyId } satisfies Holding;
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

function policyEvent(value: unknown, companyId: string): value is NativeSupplierBillPolicyEvent {
  return record(value) &&
    identifier(value.id) && value.company_id === companyId && integer(value.version, 1) &&
    mode(value.previous_mode) && mode(value.matching_mode) && identifier(value.actor_id) &&
    text(value.reason, 2000) && integer(value.created_at, Number.MIN_SAFE_INTEGER) &&
    (value.actor_name === null || text(value.actor_name, 400));
}

function policy(value: unknown, actor: WorkContext): value is NativeSupplierBillPolicy {
  if (!record(value)) return false;
  if (!mode(value.matchingMode) || !integer(value.version, 0) || typeof value.canManage !== 'boolean') return false;
  if (value.companyId !== actor.companyId) return false;
  if (!(value.updatedByName === null || text(value.updatedByName, 400))) return false;
  if (!(value.updatedAt === null || integer(value.updatedAt, Number.MIN_SAFE_INTEGER))) return false;
  if (!Array.isArray(value.events) || value.events.length > 500 || !value.events.every((event) => policyEvent(event, actor.companyId))) return false;
  if (value.purchase !== undefined) {
    if (!record(value.purchase) ||
      typeof value.purchase.newWork !== 'boolean' || typeof value.purchase.settleExisting !== 'boolean' ||
      typeof value.purchase.history !== 'boolean' || !text(value.purchase.revision, 200)) return false;
  }
  return true;
}

async function read(actor: WorkContext): Promise<NativeSupplierBillPolicy | undefined> {
  const raw = process.env.ERPX_NATIVE_PROCUREMENT_CONFIG;
  if (raw === undefined) return undefined;
  const config = settings(raw);
  const target = config.holdings.find((holding) =>
    holding.holdingSlug === actor.holdingSlug && holding.holdingId === actor.holdingId && holding.companyId === actor.companyId,
  );
  if (!target || !actor.authority || !actor.iam) return fail(409, 'WORK_CONTEXT_CHANGED');
  const incoming = await headers();
  const host = incoming.get('host');
  if (!host || new URL(target.origin).host !== host) return fail(409, 'WORK_CONTEXT_CHANGED');
  const forwarded = new Headers({
    host,
    origin: target.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192) return fail(503, 'NATIVE_PROCUREMENT_COOKIE_LIMIT');
    forwarded.set('cookie', cookie);
  }
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug) forwarded.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department') forwarded.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/supplier-bills/policy`, config.privateBaseUrl);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, { method: 'GET', headers: forwarded, signal: controller.signal, redirect: 'error', cache: 'no-store' });
  } catch { return unavailable(); }
  finally { clearTimeout(timer); }
  if (response.status === 401 || response.status === 403 || response.status === 404) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status, response.status === 401 ? 'UNAUTHENTICATED' : 'PERMISSION_REQUIRED');
  }
  if (!response.ok || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  let parsed: unknown;
  try { parsed = await response.json(); } catch { return fail(503, 'NATIVE_PROCUREMENT_RESPONSE_INVALID'); }
  if (!record(parsed) || !record(parsed.context) || !policy(parsed.data, actor)) return fail(503, 'NATIVE_PROCUREMENT_RESPONSE_INVALID');
  const context = parsed.context;
  if (context.companyId !== actor.companyId || context.assignmentId !== actor.assignmentId ||
    context.revision !== actor.authority.revision || context.assignmentVersion !== actor.authority.assignmentVersion ||
    !Array.isArray(context.permissions) ||
    !actor.iam.permissions.every((permission) => (context.permissions as unknown[]).includes(permission)))
    return fail(409, 'WORK_CONTEXT_CHANGED');
  return parsed.data;
}

/** Sole supplier-invoice policy read authority. Fails closed without config. */
export async function nativeSupplierBillPolicy(actor: WorkContext): Promise<NativeSupplierBillPolicy> {
  const data = await read(actor);
  if (data === undefined) return unavailable();
  return data;
}
