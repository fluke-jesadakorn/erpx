import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Server-only transport for the native Support evidence attachment intake.
 *
 * The browser still calls the workplace generic attachment routes; those routes
 * classify `support_ticket` and forward the exact request here. A missing
 * configuration or any configured native failure fails closed with 503; this
 * module never falls back to the retained generic TS writer.
 */

export type NativeSupportAttachmentConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;

export class NativeSupportAttachmentError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Support file service could not complete the request.');
    this.name = 'NativeSupportAttachmentError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeSupportAttachmentError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && !/[\u0000-\u001f\u007f]/.test(value);
const nullableText = (value: unknown, max = 4096): value is string | null =>
  value === null || text(value, max);
const optionalIdentifier = (value: unknown): value is string | null =>
  value === null || identifier(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalInteger = (value: unknown): value is number | null =>
  value === null || integer(value);
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const oneOf = (value: unknown, allowed: readonly string[]): value is string =>
  typeof value === 'string' && allowed.includes(value);

const purposes = ['support_evidence', 'internal_evidence', 'customer_shareable'] as const;
const componentKinds = ['record', 'action', 'line', 'event', 'message', 'publication'] as const;
const setStates = ['draft', 'bound', 'withdrawn', 'expired'] as const;
const linkStates = ['draft', 'bound', 'withdrawn', 'expired'] as const;
const sourceStates = [
  'legacy_unscanned',
  'quarantined',
  'scanning',
  'available',
  'scan_error',
  'malware_rejected',
  'destroyed',
] as const;
const scanResults = ['pending', 'running', 'clean', 'infected', 'error'] as const;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export type NativeSupportAttachmentSet = Readonly<{
  id: string;
  targetType: 'support_ticket';
  targetId: string;
  componentKind: string;
  componentRef: string;
  purposeKey: string;
  expectedTargetVersion: number;
  policyVersion: number;
  classification: string;
  requiredCount: number;
  maxFiles: number;
  state: string;
  createdAt: number;
  updatedAt: number;
  links: readonly unknown[];
}>;

export type NativeSupportAttachmentReservation = Readonly<{
  uploadIntentId: string;
  attachmentLinkId: string;
  expiresAt: number;
  replay: boolean;
}>;

export type NativeSupportAttachmentCompletion = Readonly<{
  uploadIntentId: string;
  attachmentLinkId: string;
  attachmentSetId: string;
  sourceId: string;
  state: string;
  status: string;
  replay: boolean;
  scan: Readonly<{
    attempts: number;
    lastResult: string | null;
    lastError: string | null;
    signature: string | null;
    definitionUpdatedAt: number | null;
    scannedAt: number | null;
  }>;
}>;

export type NativeSupportAttachmentScan = Readonly<{ sourceId: string; state: string }>;

type Transport = (method: 'POST' | 'PUT', url: URL, headers: Headers, body: string | Uint8Array, signal: AbortSignal) => Promise<Response>;

type Context = Row & {
  userId: string;
  holdingId: string;
  companyId: string;
  assignmentId: string;
  revision: number;
  assignmentVersion: number;
  modules: string[];
  permissions: string[];
  expiresAt: number;
};

function configuration(raw: NativeSupportAttachmentConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_SUPPORT_ATTACHMENT_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reservedPorts.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30_000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_SUPPORT_ATTACHMENT_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_SUPPORT_ATTACHMENT_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_SUPPORT_ATTACHMENT_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function context(value: unknown, actor: WorkContext, holdingSlug: string): Context {
  if (
    !exact(value, [
      'userId',
      'holdingId',
      'companyId',
      'branchId',
      'departmentId',
      'branchSlug',
      'departmentSlug',
      'assignmentId',
      'assignmentVersion',
      'revision',
      'role',
      'assignmentScopeKind',
      'modules',
      'permissions',
      'expiresAt',
    ])
  )
    return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_INVALID');
  const parsed = value as Context;
  const iam = actor.iam;
  const authority = actor.authority;
  if (!iam || !authority) return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_INVALID');
  const branchSlug = actor.scope.kind === 'holding' ? null : (actor.scope.branchSlug ?? null);
  const departmentSlug =
    actor.scope.kind === 'department' ? (actor.scope.departmentSlug ?? null) : null;
  if (
    actor.holdingSlug !== holdingSlug ||
    parsed.userId !== actor.userId ||
    parsed.holdingId !== String(actor.holdingId) ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== branchSlug ||
    parsed.departmentSlug !== departmentSlug ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== authority.assignmentVersion ||
    parsed.revision !== authority.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    parsed.expiresAt <= Date.now() ||
    !Array.isArray(parsed.modules) ||
    parsed.modules.length !== actor.modules.length ||
    !parsed.modules.every((item) => actor.modules.includes(item)) ||
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length !== iam.permissions.length ||
    !parsed.permissions.every((item) => iam.permissions.includes(item))
  )
    return failure('NATIVE_SUPPORT_ATTACHMENT_CONTEXT_CHANGED');
  return parsed;
}

function setRow(value: unknown, expected: { targetId: string; purposeKey: string; componentRef: string; expectedTargetVersion: number }): NativeSupportAttachmentSet {
  if (
    !exact(value, [
      'id',
      'targetType',
      'targetId',
      'componentKind',
      'componentRef',
      'purposeKey',
      'expectedTargetVersion',
      'policyVersion',
      'classification',
      'requiredCount',
      'maxFiles',
      'state',
      'createdAt',
      'updatedAt',
      'links',
    ]) ||
    !identifier(value.id) ||
    value.targetType !== 'support_ticket' ||
    value.targetId !== expected.targetId ||
    !oneOf(value.componentKind, componentKinds) ||
    value.componentRef !== expected.componentRef ||
    value.purposeKey !== expected.purposeKey ||
    value.expectedTargetVersion !== expected.expectedTargetVersion ||
    !integer(value.policyVersion, 1) ||
    !text(value.classification, 40) ||
    !integer(value.requiredCount) ||
    !integer(value.maxFiles, 1) ||
    !oneOf(value.state, setStates) ||
    !integer(value.createdAt) ||
    !integer(value.updatedAt) ||
    !Array.isArray(value.links)
  )
    return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_INVALID');
  return value as unknown as NativeSupportAttachmentSet;
}

function reservation(value: unknown): NativeSupportAttachmentReservation {
  if (
    !exact(value, ['uploadIntentId', 'attachmentLinkId', 'expiresAt', 'replay']) ||
    !identifier(value.uploadIntentId) ||
    !identifier(value.attachmentLinkId) ||
    !integer(value.expiresAt, 1) ||
    !boolean(value.replay)
  )
    return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_INVALID');
  return value as unknown as NativeSupportAttachmentReservation;
}

const scanShape = (value: unknown): boolean =>
  record(value) &&
  exact(value, ['attempts', 'lastResult', 'lastError', 'signature', 'definitionUpdatedAt', 'scannedAt']) &&
  integer(value.attempts) &&
  (value.lastResult === null || oneOf(value.lastResult, scanResults)) &&
  nullableText(value.lastError, 160) &&
  nullableText(value.signature, 160) &&
  optionalInteger(value.definitionUpdatedAt) &&
  optionalInteger(value.scannedAt);

function completion(value: unknown): NativeSupportAttachmentCompletion {
  if (
    !exact(value, [
      'uploadIntentId',
      'attachmentLinkId',
      'attachmentSetId',
      'sourceId',
      'state',
      'status',
      'scan',
      'replay',
    ]) ||
    !identifier(value.uploadIntentId) ||
    !identifier(value.attachmentLinkId) ||
    !identifier(value.attachmentSetId) ||
    !identifier(value.sourceId) ||
    !oneOf(value.state, sourceStates) ||
    !text(value.status, 40) ||
    !scanShape(value.scan) ||
    !boolean(value.replay)
  )
    return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_INVALID');
  return value as unknown as NativeSupportAttachmentCompletion;
}

function scanResult(value: unknown, sourceId: string): NativeSupportAttachmentScan {
  if (
    !exact(value, ['sourceId', 'state']) ||
    value.sourceId !== sourceId ||
    !oneOf(value.state, sourceStates)
  )
    return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_INVALID');
  return value as unknown as NativeSupportAttachmentScan;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected || response.status !== 200) {
    let code = 'NATIVE_SUPPORT_ATTACHMENT_UNAVAILABLE';
    let message: string | undefined;
    if ([401, 403, 404, 409, 422, 423].includes(response.status) && response.body) {
      try {
        const raw = await response.text();
        const parsed = JSON.parse(raw) as unknown;
        if (
          record(parsed) &&
          record(parsed.error) &&
          text(parsed.error.code, 80) &&
          text(parsed.error.message, 1000)
        ) {
          code = parsed.error.code;
          message = parsed.error.message;
        }
      } catch {
        // Keep the bounded unavailable code.
      }
      return failure(code, response.status, message);
    }
    void response.body?.cancel().catch(() => undefined);
    return failure(code, response.status, message);
  }
  if (
    response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json' ||
    !response.body
  )
    return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_INVALID');
  const raw = await response.text();
  if (new TextEncoder().encode(raw).length > 2 * 1024 * 1024)
    return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_LIMIT');
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_INVALID');
  }
}

export function nativeSupportAttachmentAvailable(): boolean {
  return process.env.ERPX_NATIVE_SUPPORT_ATTACHMENT_CONFIG !== undefined;
}

export function nativeSupportAttachmentConfiguration(
  raw: string,
): NativeSupportAttachmentConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeSupportAttachmentError('NATIVE_SUPPORT_ATTACHMENT_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings) ||
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_SUPPORT_ATTACHMENT_CONFIGURATION_INVALID');
  return value as unknown as NativeSupportAttachmentConfiguration;
}

export function nativeSupportAttachmentFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeSupportAttachmentError)) return undefined;
  if (error.code === 'NATIVE_SUPPORT_ATTACHMENT_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401) return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403) return { status: 403, code: error.code, message: error.message };
  if (error.status === 404) return { status: 404, code: error.code, message: error.message };
  if (error.status === 409) return { status: 409, code: error.code, message: error.message };
  if (error.status === 422) return { status: 422, code: error.code, message: error.message };
  if (error.status === 423) return { status: 423, code: error.code, message: error.message };
  if (
    error.status === 503 ||
    error.code.startsWith('NATIVE_SUPPORT_ATTACHMENT_')
  )
    return {
      status: 503,
      code: 'NATIVE_SUPPORT_ATTACHMENT_UNAVAILABLE',
      message: 'The native Support file service is unavailable.',
    };
  return undefined;
}

/** SQL classification only. It never authorizes the set; the native backend
 * re-reads and authorizes the exact target inside its SERIALIZABLE unit. */
export async function supportAttachmentSetTarget(
  database: SqlDatabase,
  companyId: string,
  setId: string,
): Promise<'support_ticket' | null> {
  const row = await database
    .prepare('SELECT target_type FROM attachment_sets WHERE id=? AND company_id=?')
    .bind(setId, companyId)
    .first<{ target_type: string }>();
  return row?.target_type === 'support_ticket' ? 'support_ticket' : null;
}

export function createNativeSupportAttachmentClient(
  raw: NativeSupportAttachmentConfiguration,
  transport: Transport = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    method: 'POST' | 'PUT',
    incoming: Pick<Headers, 'get'>,
    actor: WorkContext,
    holdingSlug: string,
    path: string,
    key: string | undefined,
    body: string | Uint8Array,
    contentType: string,
    validate: (value: unknown) => unknown,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== holdingSlug)
      return failure('NATIVE_SUPPORT_ATTACHMENT_CONTEXT_CHANGED');
    const host = incoming.get('host');
    const holding = host ? config.holdings.get(host) : undefined;
    if (!holding || holding.holdingSlug !== holdingSlug || holding.holdingId !== String(actor.holdingId))
      return failure('NATIVE_SUPPORT_ATTACHMENT_AUTHORITY_INVALID', 403);
    const origin = incoming.get('origin');
    if (!origin) return failure('ORIGIN_FORBIDDEN', 403);
    const headers = new Headers({
      host: host!,
      accept: 'application/json',
      origin,
      'content-type': contentType,
      'x-erpx-context-company': actor.companyId,
      'x-erpx-context-assignment': actor.assignmentId,
      'x-erpx-context-revision': String(actor.authority.revision),
      'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
    });
    if (key) headers.set('idempotency-key', key);
    const cookie = incoming.get('cookie');
    if (cookie) {
      if (new TextEncoder().encode(cookie).length > 8192)
        return failure('NATIVE_SUPPORT_ATTACHMENT_COOKIE_LIMIT', 503);
      headers.set('cookie', cookie);
    }
    if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
      headers.set('x-erpx-workspace-branch', actor.scope.branchSlug);
    if (actor.scope.kind === 'department' && actor.scope.departmentSlug)
      headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
    const url = new URL(path, config.base);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeout);
    const response = await transport(method, url, headers, body, controller.signal)
      .catch((error: unknown) => {
        if (error instanceof NativeSupportAttachmentError) throw error;
        return failure('NATIVE_SUPPORT_ATTACHMENT_UNAVAILABLE', 503);
      })
      .finally(() => clearTimeout(timer));
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_SUPPORT_ATTACHMENT_RESPONSE_INVALID');
    context(value.context, actor, holdingSlug);
    return validate(value.data);
  };
  const base = (holdingSlug: string) =>
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/support-attachments`;
  return {
    create: (
      incoming: Pick<Headers, 'get'>,
      actor: WorkContext,
      input: {
        key: string;
        targetId: string;
        purposeKey: (typeof purposes)[number];
        expectedTargetVersion: number;
      },
    ) => {
      if (
        !identifier(input.targetId) ||
        input.targetId.length > 180 ||
        !oneOf(input.purposeKey, purposes) ||
        !integer(input.expectedTargetVersion, 1) ||
        !/^[a-zA-Z0-9:_.-]{8,180}$/.test(input.key)
      )
        return Promise.reject(new NativeSupportAttachmentError('NATIVE_SUPPORT_ATTACHMENT_REQUEST_INVALID', 422));
      const componentRef = `support-evidence:${input.purposeKey}`;
      return run(
        'POST',
        incoming,
        actor,
        actor.holdingSlug,
        `${base(actor.holdingSlug)}/attachment-sets`,
        input.key,
        JSON.stringify({
          targetType: 'support_ticket',
          targetId: input.targetId,
          componentKind: 'action',
          componentRef,
          purposeKey: input.purposeKey,
          expectedTargetVersion: input.expectedTargetVersion,
        }),
        'application/json',
        (value) =>
          setRow(value, {
            targetId: input.targetId,
            purposeKey: input.purposeKey,
            componentRef,
            expectedTargetVersion: input.expectedTargetVersion,
          }),
      ) as Promise<NativeSupportAttachmentSet>;
    },
    reserve: (
      incoming: Pick<Headers, 'get'>,
      actor: WorkContext,
      setId: string,
      input: {
        key: string;
        filename: string;
        mediaType: string;
        bytes: number;
        caption?: string;
        supersedesLinkId?: string;
      },
    ) => {
      if (
        !identifier(setId) ||
        !text(input.filename, 255) ||
        !text(input.mediaType, 100) ||
        !integer(input.bytes, 1) ||
        input.bytes > 10 * 1024 * 1024 ||
        (input.caption !== undefined && (!text(input.caption, 1000) || input.caption.length > 1000)) ||
        (input.supersedesLinkId !== undefined && !identifier(input.supersedesLinkId)) ||
        !/^[a-zA-Z0-9:_.-]{8,180}$/.test(input.key)
      )
        return Promise.reject(new NativeSupportAttachmentError('NATIVE_SUPPORT_ATTACHMENT_REQUEST_INVALID', 422));
      return run(
        'POST',
        incoming,
        actor,
        actor.holdingSlug,
        `${base(actor.holdingSlug)}/attachment-sets/${encodeURIComponent(setId)}/files`,
        input.key,
        JSON.stringify({
          filename: input.filename.trim(),
          mediaType: input.mediaType,
          bytes: input.bytes,
          ...(input.caption ? { caption: input.caption.trim() } : {}),
          ...(input.supersedesLinkId ? { supersedesLinkId: input.supersedesLinkId } : {}),
        }),
        'application/json',
        reservation,
      ) as Promise<NativeSupportAttachmentReservation>;
    },
    complete: (
      incoming: Pick<Headers, 'get'>,
      actor: WorkContext,
      uploadId: string,
      input: { contentType: string; bytes: Uint8Array },
    ) => {
      if (!identifier(uploadId) || !text(input.contentType, 100) || input.bytes.byteLength < 1)
        return Promise.reject(new NativeSupportAttachmentError('NATIVE_SUPPORT_ATTACHMENT_REQUEST_INVALID', 422));
      return run(
        'PUT',
        incoming,
        actor,
        actor.holdingSlug,
        `${base(actor.holdingSlug)}/uploads/${encodeURIComponent(uploadId)}/content`,
        undefined,
        input.bytes,
        input.contentType,
        completion,
      ) as Promise<NativeSupportAttachmentCompletion>;
    },
    scan: (incoming: Pick<Headers, 'get'>, actor: WorkContext, uploadId: string, key: string) => {
      if (!identifier(uploadId) || !/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
        return Promise.reject(new NativeSupportAttachmentError('NATIVE_SUPPORT_ATTACHMENT_REQUEST_INVALID', 422));
      return run(
        'POST',
        incoming,
        actor,
        actor.holdingSlug,
        `${base(actor.holdingSlug)}/uploads/${encodeURIComponent(uploadId)}/scan`,
        key,
        '',
        'application/json',
        (value) => scanResult(value, `attachment-${uploadId}`),
      ) as Promise<NativeSupportAttachmentScan>;
    },
  };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'POST' | 'PUT',
  url: URL,
  headers: Headers,
  body: string | Uint8Array,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method,
        agent,
        headers: { ...Object.fromEntries(headers) },
        signal,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: status >= 200 && status <= 599 ? status : 502,
              headers: values,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}

export type NativeSupportAttachmentClient = ReturnType<typeof createNativeSupportAttachmentClient>;

/** Undefined only when no native configuration exists. A configured native
 * failure is surfaced by `withNativeSupportAttachment` and never replaced by the
 * retained generic TS writer. */
export function nativeSupportAttachmentClient(): NativeSupportAttachmentClient | undefined {
  const raw = process.env.ERPX_NATIVE_SUPPORT_ATTACHMENT_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeSupportAttachmentClient(nativeSupportAttachmentConfiguration(raw));
}

/** Run one native Support intake call, mapping a configured/missing transport
 * failure to the same 503 the route would return without configuration. */
export async function withNativeSupportAttachment<T>(
  action: (client: NativeSupportAttachmentClient) => Promise<T>,
): Promise<T> {
  const client = nativeSupportAttachmentClient();
  if (!client)
    throw new ApiError(
      503,
      'SUPPORT_ATTACHMENT_NATIVE_REQUIRED',
      'The native Support file service is required.',
    );
  try {
    return await action(client);
  } catch (error) {
    const mapped = nativeSupportAttachmentFailure(error);
    if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
    throw error;
  }
}
