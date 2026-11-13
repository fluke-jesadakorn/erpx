import 'server-only';
import { directHttp } from './native-iam-assignments';
import {
  type NativePeopleRelationshipsConfiguration,
  type NativePeopleRelationshipsContext,
  type NativePeopleRelationshipsNormalized,
  nativePeopleRelationshipsNormalized,
} from './native-people-relationships';

/**
 * Native People access transport. Account preparation, custom-role publication,
 * grants, invitations and withdrawal are owned by the Rust Identity Access
 * service; this module never mints a session, Company, assignment or
 * permission. Unconfigured deployments fail closed.
 */
export type NativePeopleAccessContext = NativePeopleRelationshipsContext;
export type NativePeopleAccessConfiguration = NativePeopleRelationshipsConfiguration;
export type NativePeopleAccessNormalized = NativePeopleRelationshipsNormalized;
export const nativePeopleAccessNormalized = nativePeopleRelationshipsNormalized;

export type NativePeopleAccessCommand =
  | { kind: 'preview'; change: unknown }
  | { kind: 'apply'; token: string; key: string };

export class NativePeopleAccessError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native People access request could not be completed.');
    this.name = 'NativePeopleAccessError';
    this.code = code;
    this.status = status;
  }
}

type Row = Record<string, unknown>;
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const closed = (value: unknown, keys: readonly string[], required: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).every((key) => keys.includes(key)) &&
  required.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const optionalIdentifier = (value: unknown): value is string | null =>
  value === null || identifier(value);
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativePeopleAccessError(code, status, message);
};

function context(value: unknown, actor: NativePeopleAccessContext): Row {
  if (!record(value)) return failure('NATIVE_PEOPLE_ACCESS_RESPONSE_INVALID');
  if (
    value.userId !== actor.userId ||
    value.holdingId !== actor.holdingId ||
    value.companyId !== actor.companyId ||
    optionalIdentifier(value.branchId) === false ||
    value.branchId !== actor.branchId ||
    optionalIdentifier(value.departmentId) === false ||
    value.departmentId !== actor.departmentId ||
    value.assignmentId !== actor.assignmentId ||
    value.assignmentVersion !== actor.authority.assignmentVersion ||
    value.revision !== actor.authority.revision ||
    value.role !== actor.role ||
    !integer(value.expiresAt, 1) ||
    !Array.isArray(value.permissions) ||
    value.permissions.length > 512 ||
    !value.permissions.every((item) => identifier(item)) ||
    value.permissions.length !== actor.iam.permissions.length ||
    !value.permissions.every((item) => actor.iam.permissions.includes(item))
  )
    return failure('NATIVE_PEOPLE_ACCESS_CONTEXT_CHANGED');
  return value;
}

function preview(value: unknown) {
  if (
    !exact(value, ['token', 'change', 'summary', 'affected', 'expiresAt']) ||
    !UUID.test(String(value.token)) ||
    !record(value.change) ||
    !text(value.summary) ||
    !Array.isArray(value.affected) ||
    !value.affected.every((item) => text(item, 500)) ||
    !integer(value.expiresAt, 1)
  )
    return failure('NATIVE_PEOPLE_ACCESS_RESPONSE_INVALID');
  return value;
}

function applied(value: unknown) {
  if (
    !closed(value, ['id', 'change', 'subjectId', 'assignmentId', 'invitationId'], ['id', 'change']) ||
    !identifier(value.id) ||
    !record(value.change) ||
    !['subjectId', 'assignmentId', 'invitationId'].every(
      (key) => !Object.hasOwn(value, key) || identifier(value[key]),
    )
  )
    return failure('NATIVE_PEOPLE_ACCESS_RESPONSE_INVALID');
  return value;
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        return failure('NATIVE_PEOPLE_ACCESS_RESPONSE_LIMIT');
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.length;
  }
  return data;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PEOPLE_ACCESS_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    const body = await boundedBytes(response, 64 * 1024);
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json') {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // Keep the bounded transport failure when the owner error is malformed.
      }
    }
    return failure(
      code ?? 'NATIVE_PEOPLE_ACCESS_UNAVAILABLE',
      response.status,
      message,
    );
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PEOPLE_ACCESS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_PEOPLE_ACCESS_RESPONSE_INVALID');
  }
}

export function nativePeopleAccessAvailable(): boolean {
  return process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG !== undefined;
}

/** Map transport failures to the browser-facing API shape. */
export function nativePeopleAccessFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePeopleAccessError)) return undefined;
  if (error.code === 'NATIVE_PEOPLE_ACCESS_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (
    error.status === 404 &&
    error.code === 'NATIVE_PEOPLE_ACCESS_UNAVAILABLE'
  )
    return {
      status: 503,
      code: 'NATIVE_PEOPLE_ACCESS_UNAVAILABLE',
      message: 'The native People access service is unavailable.',
    };
  if (error.status === 403 || error.status === 404 || error.status === 409 || error.status === 422)
    return { status: error.status, code: error.code, message: error.message };
  if (error.code === 'NATIVE_PEOPLE_ACCESS_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PEOPLE_ACCESS_REQUEST_INVALID',
      message: 'Review the access details and try again.',
    };
  return {
    status: 503,
    code: 'NATIVE_PEOPLE_ACCESS_UNAVAILABLE',
    message: 'The native People access service is unavailable.',
  };
}

function transportCall(
  transport: typeof directHttp,
  config: NativePeopleAccessNormalized,
  incoming: Pick<Headers, 'get'>,
  actor: NativePeopleAccessContext,
  path: string,
  body: string,
  key: string,
  signal: AbortSignal | undefined,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (
    !holding ||
    holding.holdingSlug !== actor.holdingSlug ||
    holding.holdingId !== actor.holdingId
  )
    return Promise.reject(new NativePeopleAccessError('NATIVE_PEOPLE_ACCESS_AUTHORITY_INVALID'));
  if (!text(key, 180) || key.length < 8 || !/^[A-Za-z0-9:_.-]+$/.test(key))
    return Promise.reject(new NativePeopleAccessError('NATIVE_PEOPLE_ACCESS_REQUEST_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'content-type': 'application/json',
    'idempotency-key': key,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativePeopleAccessError('NATIVE_PEOPLE_ACCESS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/iam/people-access${path}`,
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  return transport('POST', url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

export function createNativePeopleAccessClient(
  raw: NativePeopleAccessConfiguration,
  transport: typeof directHttp = directHttp,
) {
  const config = nativePeopleAccessNormalized(raw);
  const run = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativePeopleAccessContext,
    path: string,
    body: string,
    key: string,
    signal: AbortSignal | undefined,
    validate: (value: unknown) => unknown,
  ) => {
    if (!actor.authority || !actor.iam)
      return failure('NATIVE_PEOPLE_ACCESS_CONTEXT_CHANGED');
    const response = await transportCall(
      transport,
      config,
      incoming,
      actor,
      path,
      body,
      key,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativePeopleAccessError) throw error;
      return failure('NATIVE_PEOPLE_ACCESS_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_PEOPLE_ACCESS_RESPONSE_INVALID');
    context(value.context, actor);
    return validate(value.data);
  };
  return {
    preview: (
      incoming: Pick<Headers, 'get'>,
      actor: NativePeopleAccessContext,
      change: unknown,
      signal?: AbortSignal,
    ) =>
      run(
        incoming,
        actor,
        '/preview',
        JSON.stringify({ change }),
        `people-access-preview-${crypto.randomUUID()}`,
        signal,
        preview,
      ),
    apply: (
      incoming: Pick<Headers, 'get'>,
      actor: NativePeopleAccessContext,
      token: string,
      key: string,
      signal?: AbortSignal,
    ) => {
      if (!UUID.test(token))
        return Promise.reject(new NativePeopleAccessError('NATIVE_PEOPLE_ACCESS_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        '/apply',
        JSON.stringify({ previewToken: token }),
        key,
        signal,
        applied,
      );
    },
  };
}
