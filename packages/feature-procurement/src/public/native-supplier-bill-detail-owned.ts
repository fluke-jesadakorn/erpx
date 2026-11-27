import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { requireProcurementPurchasedRead } from '@erpx/server-procurement/public/purchases';
import { nativeProcurementAvailable } from './native-supplier-promise-history';

/**
 * Fail-closed client for the native Procurement supplier-bill detail route.
 * The axum route executes all owner SQL; this transport carries no SQL and has
 * no TypeScript fallback. A missing native configuration is an error, never a
 * retained TypeScript reader.
 */
type Row = Record<string, unknown>;
export type NativeSupplierBillDetail = Readonly<{
  bill: Row;
  lines: Row[];
  receipts: Row[];
  events: Row[];
  accounts: Row[];
  canReview: boolean;
  canPrepare: boolean;
  purchase?: {newWork: boolean; settleExisting: boolean; history: boolean; revision: string};
}>;

const fail = (status: number, code: string, message = 'The native Procurement bill detail is unavailable.'): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (): never => fail(503, 'NATIVE_PROCUREMENT_UNAVAILABLE');
const record = (value: unknown): value is Row => !!value && typeof value === 'object' && !Array.isArray(value);
const rows = (value: unknown): value is Row[] => Array.isArray(value) && value.length <= 10_001 && value.every(record);
const identifier = (value: unknown): value is string => typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 200;

function detail(value: unknown, companyId: string, billId: string): value is NativeSupplierBillDetail {
  if (!record(value)) return false;
  if (!record(value.bill) || value.bill.id !== billId || value.bill.company_id !== companyId) return false;
  if (!rows(value.lines) || !rows(value.receipts) || !rows(value.events) || !rows(value.accounts)) return false;
  if (typeof value.canReview !== 'boolean' || typeof value.canPrepare !== 'boolean') return false;
  if (value.purchase !== undefined) {
    if (!record(value.purchase) || typeof value.purchase.newWork !== 'boolean' ||
      typeof value.purchase.settleExisting !== 'boolean' || typeof value.purchase.history !== 'boolean' ||
      !identifier(value.purchase.revision)) return false;
  }
  return true;
}

type Holding = Readonly<{origin: string; holdingSlug: string; holdingId: string; companyId: string}>;
type Settings = Readonly<{privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[]}>;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function settings(raw: string): Settings {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    value.privateBaseUrl !== base.origin) return unavailable();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' ||
      typeof item.holdingId !== 'string' || typeof item.companyId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password ||
      !identifier(item.companyId) || !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug)) return unavailable();
    return {origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId, companyId: item.companyId} as Holding;
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return {privateBaseUrl: base.origin, timeoutMs: value.timeoutMs as number, holdings};
}

/** Sole supplier-bill detail reader once the native owner route is configured. */
export async function nativeSupplierBillDetail(actor: WorkContext, billId: string): Promise<NativeSupplierBillDetail> {
  const raw = process.env.ERPX_NATIVE_PROCUREMENT_CONFIG;
  if (raw === undefined) return unavailable();
  const config = settings(raw);
  const target = config.holdings.find(holding =>
    holding.holdingSlug === actor.holdingSlug && holding.holdingId === actor.holdingId && holding.companyId === actor.companyId);
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
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/supplier-bills/${encodeURIComponent(billId)}`, config.privateBaseUrl);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, {method: 'GET', headers: forwarded, signal: controller.signal, redirect: 'error', cache: 'no-store'});
  } catch { return unavailable(); }
  finally { clearTimeout(timer); }
  if ([401, 403, 404].includes(response.status)) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status, response.status === 401 ? 'UNAUTHENTICATED' : 'PERMISSION_REQUIRED');
  }
  if (!response.ok || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  let parsed: unknown;
  try { parsed = await response.json(); } catch { return fail(503, 'NATIVE_PROCUREMENT_RESPONSE_INVALID'); }
  if (!record(parsed) || !record(parsed.context) || !detail(parsed.data, actor.companyId, billId)) {
    return fail(503, 'NATIVE_PROCUREMENT_RESPONSE_INVALID');
  }
  const context = parsed.context;
  if (context.companyId !== actor.companyId || context.assignmentId !== actor.assignmentId ||
    context.revision !== actor.authority.revision || context.assignmentVersion !== actor.authority.assignmentVersion ||
    !Array.isArray(context.permissions) || !actor.iam.permissions.every(permission => (context.permissions as unknown[]).includes(permission))) {
    return fail(409, 'WORK_CONTEXT_CHANGED');
  }
  return parsed.data;
}

/** Keep the retained module and purchased-package preflight ahead of the
 * native read. The owner still re-checks the actor's current Company scope;
 * once configured, a failure never falls back to a TypeScript bill reader. */
async function admission(actor: WorkContext) {
  if (!actor.modules.includes('procurement'))
    throw new ApiError(403, 'MODULE_FORBIDDEN', 'Your current role cannot access Procurement.');
  try {
    const database = (await holdingResources(actor.holdingId)).write;
    await requireProcurementPurchasedRead(database, actor);
  } catch (error) {
    if (error instanceof ApiError) throw error;
    throw new ApiError(
      503,
      'NATIVE_PROCUREMENT_UNAVAILABLE',
      'The native Procurement service is unavailable.',
    );
  }
}

/** Native supplier-bill detail port for the Supply API route and the SSR
 * presenter. The caller must fail closed when the owner is not configured;
 * an undefined result is never an authorization to read TypeScript. */
export async function ownedSupplierBillDetail(
  actor: WorkContext,
  holdingSlug: string,
  billId: string,
): Promise<NativeSupplierBillDetail | undefined> {
  if (!nativeProcurementAvailable()) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new ApiError(
      409,
      'WORK_CONTEXT_CHANGED',
      'Your work role changed. Reload and review before retrying.',
    );
  await admission(actor);
  return nativeSupplierBillDetail(actor, billId);
}
