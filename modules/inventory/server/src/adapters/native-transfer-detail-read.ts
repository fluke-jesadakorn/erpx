import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { TransferRow } from '@erpx/inventory-server/domain/transfers';

/**
 * Sole warehouse-transfer detail read authority.
 *
 * The mounted owner route
 * `/api/v1/holdings/<holding>/inventory/native-transfer-detail/read` (POST)
 * executes the retained owner SQL. This transport never prepares or executes
 * SQL: `ERPX_NATIVE_INVENTORY_COMMAND_CONFIG` selects the private service and a
 * request without a registered Holding target fails closed. A configured read
 * that fails never falls back to the retired TypeScript detail query.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Holding[];
  production: boolean;
}>;

const reservedPorts = new Set([3000, 3001, 3100, 3103, 5432, 5433, 5435, 8080]);
const fail = (status: number, code: string, message: string): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (): never =>
  fail(503, 'NATIVE_INVENTORY_TRANSFER_DETAIL_UNAVAILABLE', 'The native Inventory transfer reader is unavailable.');
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const scalar = (value: unknown): value is string | number | null =>
  value === null || typeof value === 'string' || (typeof value === 'number' && Number.isFinite(value));
const identity = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length > 0 &&
  value.length <= 200 &&
  /^[a-z0-9._:-]+$/i.test(value);

function settings(raw: string): Settings {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable();
  }
  if (
    !record(value) ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    (value.production !== undefined && typeof value.production !== 'boolean') ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  ) {
    return unavailable();
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable();
  }
  const port = Number(base.port);
  const production = value.production === true;
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reservedPorts.has(port) ||
    (port >= 3200 && port <= 3299) ||
    (port === 3101 && !production) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  ) {
    return unavailable();
  }
  const holdings = value.holdings.map((item: unknown): Holding => {
    if (
      !record(item) ||
      typeof item.origin !== 'string' ||
      typeof item.holdingSlug !== 'string' ||
      typeof item.holdingId !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug) ||
      !identity(item.holdingId)
    ) {
      return unavailable();
    }
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable();
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      !origin.hostname.includes('.')
    ) {
      return unavailable();
    }
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  if (new Set(holdings.map((holding) => holding.origin)).size !== holdings.length) return unavailable();
  if (new Set(holdings.map((holding) => holding.holdingSlug)).size !== holdings.length) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings, production };
}

function row(value: unknown, companyId: string): TransferRow | null | undefined {
  if (!record(value) || value.company_id !== companyId) return undefined;
  if (!Object.values(value).every(scalar)) return undefined;
  return value as TransferRow;
}

/** Sole transfer-detail read authority. Fails closed without a native target.
 *  A not-found native reply returns `null`, matching the retired adapter's
 *  missing-row path; every other failure throws with no TypeScript fallback. */
export async function nativeTransferDetail(actor: WorkContext, id: string): Promise<TransferRow | null> {
  const raw = process.env.ERPX_NATIVE_INVENTORY_COMMAND_CONFIG;
  if (raw === undefined) return unavailable();
  const config = settings(raw);
  const target = config.holdings.find(
    (holding) => holding.holdingSlug === actor.holdingSlug && holding.holdingId === actor.holdingId,
  );
  if (!target || !identity(actor.companyId) || !identity(id)) {
    return fail(409, 'WORK_CONTEXT_CHANGED', 'The work context changed.');
  }
  if (actor.branchId !== null && !identity(actor.branchId)) {
    return fail(409, 'WORK_CONTEXT_CHANGED', 'The work context changed.');
  }
  const incoming = await headers();
  const host = incoming.get('host');
  if (!host || new URL(target.origin).host !== host) {
    return fail(409, 'WORK_CONTEXT_CHANGED', 'The work context changed.');
  }
  const forwarded = new Headers({
    host,
    origin: target.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
  });
  if (actor.branchId !== null) forwarded.set('x-erpx-workspace-branch', actor.branchId);
  if (actor.scope.kind === 'department') forwarded.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  for (const [name, value] of [
    ['cookie', incoming.get('cookie')],
    ['x-erpx-context-assignment', actor.iam ? actor.assignmentId : null],
    ['x-erpx-context-revision', actor.authority ? String(actor.authority.revision) : null],
    ['x-erpx-context-assignment-version', actor.authority ? String(actor.authority.assignmentVersion) : null],
  ] as const) {
    if (value !== null) forwarded.set(name, value);
  }
  const cookie = forwarded.get('cookie');
  if (cookie && new TextEncoder().encode(cookie).length > 8192) {
    return fail(503, 'NATIVE_INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID', 'The transfer read request is too large.');
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/inventory/native-transfer-detail/read`,
    config.privateBaseUrl,
  );
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: forwarded,
      body: JSON.stringify({ id }),
      signal: controller.signal,
      redirect: 'error',
      cache: 'no-store',
    });
  } catch {
    return unavailable();
  } finally {
    clearTimeout(timer);
  }
  if (response.status === 404) {
    void response.body?.cancel().catch(() => undefined);
    return null;
  }
  if (response.status === 401 || response.status === 403) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status, response.status === 401 ? 'UNAUTHENTICATED' : 'PERMISSION_REQUIRED', 'The transfer read was rejected.');
  }
  if (!response.ok || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  let parsed: unknown;
  try {
    parsed = await response.json();
  } catch {
    return fail(503, 'NATIVE_INVENTORY_TRANSFER_DETAIL_RESPONSE_INVALID', 'The native transfer response is invalid.');
  }
  const result = row(parsed, actor.companyId);
  if (result === undefined) {
    return fail(503, 'NATIVE_INVENTORY_TRANSFER_DETAIL_RESPONSE_INVALID', 'The native transfer response is invalid.');
  }
  return result;
}
