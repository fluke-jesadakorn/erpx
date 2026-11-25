import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict private transport for the native Legal executed-copy read. The axum
 * owner executes all five statements and owns the Company scope, projection and
 * response shape; this bridge emits no SQL and accepts no SQL from a caller.
 * Missing configuration, an unregistered target, a transport failure or a
 * malformed payload all fail closed with no TypeScript fallback.
 */

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

/** The bounded slice of a signed-in WorkContext used for Holding selection.
 * A full WorkContext satisfies this structurally; optional local scopes let
 * the owner tests exercise a minimal actor without fabricating an identity. */
export type NativeLane26Actor = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  userId: string;
  branchId?: string | null;
  departmentId?: string | null;
}>;

export type NativeLane26Scope = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  userId: string;
  branchId?: string | null;
  departmentId?: string | null;
}>;

export type NativeExecutedCopyReview = Readonly<{
  id: string;
  executed_copy_id: string;
  item_version: number;
  decision: string;
  checks: Record<string, unknown>;
  reason: string;
  reviewer_id: string;
  created_at: number;
}>;

export type NativeExecutedCopy = Readonly<{
  id: string;
  item_id: string;
  document_id: string;
  approved_cycle: number;
  approved_document_id: string;
  approved_hash: string;
  executed_hash: string;
  metadata: Record<string, unknown>;
  state: 'pending_review' | 'changes_required' | 'confirmed';
  imported_by: string;
  imported_at: number;
  reviewed_by: string | null;
  reviewed_at: number | null;
  version: number;
  document_name: string;
  document_version: number;
  document_author: string;
  approved_author: string;
  reviews: readonly NativeExecutedCopyReview[];
}>;

export type NativeExecutedCopyRead = Readonly<{
  item_id: string;
  member: boolean;
  approval: { cycle: number } | null;
  copies: readonly NativeExecutedCopy[];
}>;

const ENV = 'ERPX_NATIVE_LEGAL_LANE26_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = 'NATIVE_LEGAL_COPY_READ_UNAVAILABLE', status = 503): never => {
  throw new ApiError(status, code, 'The native Legal executed-copy service is unavailable.');
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const safeInteger = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value);
const nullableText = (value: unknown, max = 8192): value is string | null =>
  value === null || text(value, max);

/** Derive the bounded transport scope from the signed-in owner context. */
export function nativeLane26Scope(actor: NativeLane26Actor): NativeLane26Scope {
  return {
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    companyId: actor.companyId,
    userId: actor.userId,
  };
}

function configuration(): Config {
  const raw = process.env[ENV];
  if (!raw) return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_INVALID');
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl, 512) ||
    !safeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl as string);
  } catch {
    return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_INVALID');
  const holdings: Holding[] = [];
  const seenTargets = new Set<string>();
  const seenHosts = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      (item.companyId !== undefined && !text(item.companyId, 200)) ||
      !/^[a-z0-9_-]+$/.test(String(item.holdingSlug)) ||
      !/^[A-Za-z0-9._:-]+$/.test(String(item.host))
    )
      return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_INVALID');
    }
    if (origin.protocol !== 'https:' || origin.origin !== item.origin || origin.username || origin.password)
      return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_INVALID');
    // The exact host is the registered router key; it must agree with the
    // registered origin and be unique across every Holding in this config.
    if (String(item.host) !== origin.host) return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_INVALID');
    const targetKey = `${String(item.holdingId)}\u0000${String(item.holdingSlug)}`;
    if (seenTargets.has(targetKey) || seenHosts.has(String(item.host)))
      return unavailable('NATIVE_LEGAL_COPY_READ_CONFIGURATION_INVALID');
    seenTargets.add(targetKey);
    seenHosts.add(String(item.host));
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

function isReview(value: unknown): value is NativeExecutedCopyReview {
  return (
    object(value) &&
    text(value.id, 200) &&
    text(value.executed_copy_id, 200) &&
    safeInteger(value.item_version) &&
    text(value.decision, 100) &&
    object(value.checks) &&
    text(value.reason, 8192) &&
    text(value.reviewer_id, 200) &&
    safeInteger(value.created_at)
  );
}

function isCopy(value: unknown): value is NativeExecutedCopy {
  return (
    object(value) &&
    text(value.id, 200) &&
    text(value.item_id, 200) &&
    text(value.document_id, 200) &&
    safeInteger(value.approved_cycle) &&
    text(value.approved_document_id, 200) &&
    text(value.approved_hash, 512) &&
    text(value.executed_hash, 512) &&
    object(value.metadata) &&
    ['pending_review', 'changes_required', 'confirmed'].includes(String(value.state)) &&
    text(value.imported_by, 200) &&
    safeInteger(value.imported_at) &&
    nullableText(value.reviewed_by, 200) &&
    (value.reviewed_at === null || safeInteger(value.reviewed_at)) &&
    safeInteger(value.version) &&
    text(value.document_name, 8192) &&
    safeInteger(value.document_version) &&
    text(value.document_author, 200) &&
    text(value.approved_author, 200) &&
    Array.isArray(value.reviews) &&
    value.reviews.every(isReview)
  );
}

function isRead(value: unknown): value is NativeExecutedCopyRead {
  return (
    object(value) &&
    text(value.item_id, 200) &&
    typeof value.member === 'boolean' &&
    (value.approval === null ||
      (object(value.approval) && safeInteger(value.approval.cycle))) &&
    Array.isArray(value.copies) &&
    value.copies.every(isCopy)
  );
}

/** Read the complete owner-local executed-copy projection for one item. The
 * registered Holding is selected from the caller's WorkContext; an unknown or
 * ambiguous actor never falls back to another Holding. */
export async function nativeExecutedCopiesRead(
  actor: NativeLane26Actor,
  itemId: string,
): Promise<NativeExecutedCopyRead> {
  const scope = nativeLane26Scope(actor);
  if (
    !text(scope.holdingId, 200) ||
    !text(scope.holdingSlug, 128) ||
    !/^[a-z0-9_-]+$/.test(scope.holdingSlug) ||
    !text(scope.companyId, 200) ||
    !text(scope.userId, 200) ||
    (scope.branchId != null && !text(scope.branchId, 200)) ||
    (scope.departmentId != null && !text(scope.departmentId, 200))
  )
    return unavailable('NATIVE_LEGAL_COPY_AUTHORITY_INVALID');
  const config = configuration();
  const target = config.holdings.find(
    (holding) =>
      holding.holdingId === scope.holdingId &&
      holding.holdingSlug === scope.holdingSlug &&
      (holding.companyId === undefined || holding.companyId === scope.companyId),
  );
  if (!target) return unavailable('NATIVE_LEGAL_COPY_AUTHORITY_INVALID');
  if (!itemId || itemId.length > 200 || !/^[A-Za-z0-9._:-]+$/.test(itemId))
    return unavailable('NATIVE_LEGAL_COPY_READ_REQUEST_INVALID');

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(scope.holdingSlug)}/legal/native-lane-26/executed-copies/${encodeURIComponent(itemId)}`,
    config.base,
  );
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': scope.companyId,
    'x-erpx-context-user': scope.userId,
  });
  if (!headers.get('x-erpx-context-user'))
    return unavailable('NATIVE_LEGAL_COPY_AUTHORITY_INVALID', 409);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers: Object.fromEntries(headers),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    const payload = (await response.json().catch((): unknown => null)) as
      | { error?: { code?: string; message?: string } }
      | null;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(payload?.error?.code, 100) ? String(payload!.error!.code) : 'NATIVE_LEGAL_COPY_READ_FAILED',
        text(payload?.error?.message, 300)
          ? String(payload!.error!.message)
          : 'The native Legal executed-copy read was rejected.',
      );
    return unavailable();
  }
  const payload: unknown = await response.json().catch((): unknown => null);
  if (!isRead(payload)) return unavailable('NATIVE_LEGAL_COPY_READ_RESPONSE_INVALID');
  return payload;
}

/** True when the owner consumer switch is configured. */
export function nativeLane26Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
