import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { directHttp } from './native-crm-pipeline';

type Row = Record<string, unknown>;

export class NativeEcommerceListingError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Ecommerce listing command could not be completed.');
    this.name = 'NativeEcommerceListingError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeEcommerceListingError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};

const LISTING_ACTIONS = new Set(['publish', 'reject', 'retire', 'activate']);
const STATUSES = new Set(['draft', 'published', 'rejected', 'retired']);
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

type Config = {
  base: URL;
  timeoutMs: number;
  holdings: Map<string, { origin: string; holdingSlug: string; holdingId: string }>;
};

type Operation =
  | { kind: 'create'; body: Row; id: string }
  | { kind: 'change'; body: Row; id: string }
  | { kind: 'review'; body: Row; id: string };

/** Whether the route must use the native listing slice. The retained TS
 * createListing/changeEcommerce writers for this entity are gone, so an absent
 * native transport fails closed instead of falling back. */
export function nativeEcommerceListingRequired(input: unknown): boolean {
  return record(input) && input.entity === 'listing';
}

export function nativeEcommerceListingAvailable(): boolean {
  return (
    process.env.ERPX_NATIVE_ECOMMERCE_LISTING_CONFIG !== undefined ||
    process.env.ERPX_NATIVE_ECOMMERCE_CATALOG_CONFIG !== undefined
  );
}

function config(raw: string): Config {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return failure('NATIVE_ECOMMERCE_LISTING_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs, 250) ||
    value.timeoutMs > 30000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return failure('NATIVE_ECOMMERCE_LISTING_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return failure('NATIVE_ECOMMERCE_LISTING_CONFIGURATION_INVALID');
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
    value.privateBaseUrl !== base.origin
  )
    return failure('NATIVE_ECOMMERCE_LISTING_CONFIGURATION_INVALID');
  const holdings = new Map<string, { origin: string; holdingSlug: string; holdingId: string }>();
  for (const holding of value.holdings) {
    if (
      !record(holding) ||
      !text(holding.origin) ||
      !text(holding.holdingSlug, 128) ||
      !text(holding.holdingId, 200) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      !identifier(holding.holdingId)
    )
      return failure('NATIVE_ECOMMERCE_LISTING_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_ECOMMERCE_LISTING_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_ECOMMERCE_LISTING_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      origin: holding.origin,
      holdingSlug: holding.holdingSlug,
      holdingId: holding.holdingId,
    });
  }
  return { base, timeoutMs: value.timeoutMs, holdings };
}

function configuration(): Config | undefined {
  const raw =
    process.env.ERPX_NATIVE_ECOMMERCE_LISTING_CONFIG ??
    process.env.ERPX_NATIVE_ECOMMERCE_CATALOG_CONFIG;
  return raw === undefined ? undefined : config(raw);
}

function operation(input: unknown, key: string): Operation | undefined {
  if (!record(input) || input.entity !== 'listing') return undefined;
  const id = input.id;
  if (typeof id === 'string') {
    if (!identifier(id)) return failure('NATIVE_ECOMMERCE_LISTING_REQUEST_INVALID', 422);
    if (typeof input.action === 'string') {
      if (!LISTING_ACTIONS.has(input.action))
        return failure('NATIVE_ECOMMERCE_LISTING_REQUEST_INVALID', 422);
      return {
        kind: 'review',
        id,
        body: {
          kind: 'listing',
          id,
          action: input.action,
          expectedVersion: positive(input.expectedVersion),
          reason: requiredText(input.reason),
        },
      };
    }
    return {
      kind: 'change',
      id,
      body: {
        expectedVersion: positive(input.expectedVersion),
        title: requiredText(input.title),
      },
    };
  }
  if (
    !identifier(input.channelId) ||
    !identifier(input.productId) ||
    !text(input.title, 200) ||
    input.title.trim().length === 0 ||
    !text(input.currency, 3) ||
    !/^[A-Za-z]{3}$/.test(input.currency) ||
    !text(input.price, 40) ||
    input.price.trim().length === 0
  )
    return failure('NATIVE_ECOMMERCE_LISTING_REQUEST_INVALID', 422);
  return {
    kind: 'create',
    id: `ecommerce-listing-${key}`,
    body: {
      channelId: input.channelId,
      productId: input.productId,
      title: input.title,
      currency: input.currency,
      price: input.price,
    },
  };
}

function positive(value: unknown): number {
  if (!integer(value, 1)) return failure('NATIVE_ECOMMERCE_LISTING_REQUEST_INVALID', 422);
  return value;
}

function requiredText(value: unknown): string {
  if (!text(value, 2000) || (value as string).trim().length === 0)
    return failure('NATIVE_ECOMMERCE_LISTING_REQUEST_INVALID', 422);
  return value as string;
}

function authority(actor: WorkContext): {
  assignmentId: string;
  revision: number;
  assignmentVersion: number;
} {
  if (!actor.authority || !actor.iam)
    return failure('NATIVE_ECOMMERCE_LISTING_CONTEXT_CHANGED', 409);
  return {
    assignmentId: actor.assignmentId,
    revision: actor.authority.revision,
    assignmentVersion: actor.authority.assignmentVersion,
  };
}

function scope(actor: WorkContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function context(value: unknown, actor: WorkContext, expected: ReturnType<typeof authority>) {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_ECOMMERCE_LISTING_RESPONSE_INVALID');
  const parsed = value;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== (scope(actor).branchSlug ?? null) ||
    parsed.departmentSlug !== (scope(actor).departmentSlug ?? null) ||
    parsed.assignmentId !== expected.assignmentId ||
    parsed.assignmentVersion !== expected.assignmentVersion ||
    parsed.revision !== expected.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    !texts(parsed.modules) ||
    !sameSet(actor.modules, parsed.modules) ||
    !texts(parsed.permissions) ||
    !sameSet(actor.iam!.permissions, parsed.permissions)
  )
    return failure('NATIVE_ECOMMERCE_LISTING_CONTEXT_CHANGED', 409);
}

function receipt(value: Row, op: Operation): Row {
  if (
    !exact(value.data, ['id', 'status', 'version']) ||
    value.data.id !== op.id ||
    typeof value.data.status !== 'string' ||
    !STATUSES.has(value.data.status) ||
    !integer(value.data.version, 1)
  )
    return failure('NATIVE_ECOMMERCE_LISTING_RESPONSE_INVALID');
  return value.data;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_ECOMMERCE_LISTING_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  const bytes = response.body ? new Uint8Array(await response.arrayBuffer()) : new Uint8Array();
  if (response.status !== 200) {
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json' && bytes.byteLength <= 64 * 1024) {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_ECOMMERCE_LISTING_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_ECOMMERCE_LISTING_RESPONSE_INVALID');
  }
  if (bytes.byteLength > 256 * 1024)
    return failure('NATIVE_ECOMMERCE_LISTING_RESPONSE_INVALID');
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch {
    return failure('NATIVE_ECOMMERCE_LISTING_RESPONSE_INVALID');
  }
}

export type NativeEcommerceListingTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

/**
 * Send one owner-complete native Ecommerce listing command. Returns
 * `undefined` only when the command is outside this slice or the transport is
 * unconfigured; callers use `nativeEcommerceListingRequired` to fail closed on
 * a listing command instead of reaching the retired TypeScript writer.
 */
export async function nativeEcommerceListingCommand(
  actor: WorkContext,
  holdingSlug: string,
  input: unknown,
  key: string,
  incoming: Pick<Headers, 'get'>,
  transport: NativeEcommerceListingTransport = directHttp,
): Promise<Row | undefined> {
  const cfg = configuration();
  if (!cfg) return undefined;
  const op = operation(input, key);
  if (!op) return undefined;
  const expected = authority(actor);
  const host = incoming.get('host');
  const holding = host
    ? [...cfg.holdings.values()].find(
        (value) =>
          new URL(value.origin).host === host &&
          value.holdingSlug === holdingSlug &&
          value.holdingId === actor.holdingId,
      )
    : undefined;
  if (!holding) return failure('NATIVE_ECOMMERCE_LISTING_AUTHORITY_INVALID', 403);
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': key,
  });
  const origin = incoming.get('origin');
  if (origin) headers.set('origin', origin);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return failure('NATIVE_ECOMMERCE_LISTING_COOKIE_LIMIT', 422);
    headers.set('cookie', cookie);
  }
  for (const [name, slug] of [
    ['x-erpx-workspace-branch', scope(actor).branchSlug],
    ['x-erpx-workspace-department', scope(actor).departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return failure('NATIVE_ECOMMERCE_LISTING_REQUEST_INVALID', 422);
      headers.set(name, slug);
    }
  }
  headers.set('x-erpx-context-company', actor.companyId);
  headers.set('x-erpx-context-assignment', expected.assignmentId);
  headers.set('x-erpx-context-revision', String(expected.revision));
  headers.set('x-erpx-context-assignment-version', String(expected.assignmentVersion));
  const path =
    op.kind === 'create'
      ? `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/ecommerce/listings`
      : op.kind === 'change'
        ? `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/ecommerce/listings/${encodeURIComponent(op.id)}/changes`
        : `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/ecommerce/reviews`;
  const url = new URL(path, cfg.base);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), cfg.timeoutMs);
  try {
    const response = await transport(
      'POST',
      url,
      headers,
      JSON.stringify(op.body),
      controller.signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeEcommerceListingError) throw error;
      return failure('NATIVE_ECOMMERCE_LISTING_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!record(value)) return failure('NATIVE_ECOMMERCE_LISTING_RESPONSE_INVALID');
    context(value.context, actor, expected);
    return receipt(value, op);
  } finally {
    clearTimeout(timer);
  }
}

/** Bounded failure policy for the retained route error handling. */
export function nativeEcommerceListingFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeEcommerceListingError)) return undefined;
  if (error.code === 'NATIVE_ECOMMERCE_LISTING_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403 || error.status === 404)
    return { status: error.status, code: 'NOT_FOUND', message: 'Not found.' };
  if (error.status === 409 || error.status === 422)
    return { status: error.status, code: error.code, message: error.message };
  return {
    status: 503,
    code: 'NATIVE_ECOMMERCE_LISTING_UNAVAILABLE',
    message: 'The native Ecommerce listing service is temporarily unavailable.',
  };
}

/** Route-facing wrapper that maps native failures to the retained policy. */
export async function nativeEcommerceListingOrThrow(
  request: Request,
  actor: WorkContext,
  holdingSlug: string,
  input: unknown,
  key: string,
): Promise<Row | undefined> {
  try {
    return await nativeEcommerceListingCommand(actor, holdingSlug, input, key, request.headers);
  } catch (error) {
    const policy = nativeEcommerceListingFailure(error);
    if (policy) throw new ApiError(policy.status, policy.code, policy.message);
    throw error;
  }
}
