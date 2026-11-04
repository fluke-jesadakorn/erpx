import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeAttachmentPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  scope: Readonly<{
    kind: 'holding' | 'branch' | 'department';
    branchSlug?: string;
    departmentSlug?: string;
  }>;
  authority: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam: { permissions: readonly string[] };
};

export type NativeAttachmentConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativeAttachmentSetRow = Readonly<{
  id: string;
  targetType: string;
  targetId: string;
  componentKind: string;
  componentRef: string | null;
  purposeKey: string;
  expectedTargetVersion: number;
  policyVersion: number;
  classification: string;
  requiredCount: number;
  maxFiles: number;
  state: string;
  createdAt: number;
  updatedAt: number;
  sanitizationAttested?: boolean;
  links: readonly NativeAttachmentLinkRow[];
}>;

export type NativeAttachmentLinkRow = Readonly<{
  id: string;
  sourceId: string | null;
  filename: string;
  mediaType: string;
  bytes: number | null;
  sourceState: string | null;
  linkState: string;
  caption: string | null;
  previewAvailable: boolean;
  createdAt: number;
  expiresAt: number;
  boundAt: number | null;
}>;

export type NativeAttachmentStatusRow = Readonly<{
  linkId: string;
  setId: string;
  sourceId: string | null;
  sourceState: string | null;
  linkState: string;
  scan: Readonly<{
    attempts: number;
    lastResult: string | null;
    lastError: string | null;
    signature: string | null;
    definitionUpdatedAt: number | null;
    scannedAt: number | null;
  }>;
  previewAvailable: boolean;
  bindable: boolean;
  expiresAt: number;
  boundAt: number | null;
  withdrawnAt: number | null;
}>;

export type NativeAttachmentListInput = Readonly<{
  holdingSlug: string;
  targetType: string;
  targetId: string;
  purposeKey?: string;
  componentKind?: string;
  expectedTargetVersion?: number;
  readOnly?: boolean;
}>;

export type NativeAttachmentReadTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

type Row = Record<string, unknown>;
type Context = Row & {
  branchSlug: string | null;
  departmentSlug: string | null;
  permissions: string[];
  modules: string[];
};

export class NativeAttachmentReadError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native file reader could not complete the request.');
    this.name = 'NativeAttachmentReadError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeAttachmentReadError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= maximum;
const identifier = (value: unknown): value is string =>
  text(value, 200) && !/[\u0000-\u001f\u007f]/.test(value);
const optionalIdentifier = (value: unknown): value is string | null =>
  value === null || identifier(value);
const nullableText = (value: unknown, maximum = 4096): value is string | null =>
  value === null || text(value, maximum);
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
const optionalInteger = (value: unknown): value is number | null =>
  value === null || integer(value);
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const oneOf = (value: unknown, allowed: readonly string[]): value is string =>
  typeof value === 'string' && allowed.includes(value);

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
const classifications = ['public', 'internal', 'confidential', 'restricted'] as const;
const scanResults = ['pending', 'running', 'clean', 'infected', 'error'] as const;
const sanitizationTargets = new Set([
  'iam_review',
  'iam_migration',
  'company_release_acceptance',
  'people_provision_operation',
  'company_provisioning',
]);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function link(value: unknown): NativeAttachmentLinkRow {
  if (
    !exact(value, [
      'id',
      'sourceId',
      'filename',
      'mediaType',
      'bytes',
      'sourceState',
      'linkState',
      'caption',
      'previewAvailable',
      'createdAt',
      'expiresAt',
      'boundAt',
    ]) ||
    !identifier(value.id) ||
    !optionalIdentifier(value.sourceId) ||
    !text(value.filename, 255) ||
    !text(value.mediaType, 100) ||
    !(value.bytes === null || integer(value.bytes, 1)) ||
    !(value.sourceState === null || oneOf(value.sourceState, sourceStates)) ||
    !oneOf(value.linkState, linkStates) ||
    !nullableText(value.caption, 1000) ||
    !boolean(value.previewAvailable) ||
    !integer(value.createdAt) ||
    !integer(value.expiresAt) ||
    value.expiresAt < value.createdAt ||
    !optionalInteger(value.boundAt) ||
    (value.linkState === 'bound') !== (value.boundAt !== null) ||
    (value.sourceId !== null && value.sourceState === null)
  )
    return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  return value as unknown as NativeAttachmentLinkRow;
}

function set(value: unknown, input: NativeAttachmentListInput): NativeAttachmentSetRow {
  const required = sanitizationTargets.has(input.targetType)
    ? [
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
        'sanitizationAttested',
        'links',
      ]
    : [
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
      ];
  if (!exact(value, required)) return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  if (
    !identifier(value.id) ||
    value.targetType !== input.targetType ||
    value.targetId !== input.targetId ||
    !oneOf(value.componentKind, componentKinds) ||
    !nullableText(value.componentRef, 180) ||
    !text(value.purposeKey, 80) ||
    !integer(value.expectedTargetVersion, 1) ||
    !integer(value.policyVersion, 1) ||
    !oneOf(value.classification, classifications) ||
    !integer(value.requiredCount) ||
    value.requiredCount > 100 ||
    !integer(value.maxFiles, 1) ||
    value.maxFiles > 100 ||
    value.requiredCount > value.maxFiles ||
    !oneOf(value.state, setStates) ||
    !integer(value.createdAt) ||
    !integer(value.updatedAt) ||
    value.updatedAt < value.createdAt ||
    !Array.isArray(value.links) ||
    (sanitizationTargets.has(input.targetType) && !boolean(value.sanitizationAttested)) ||
    (!sanitizationTargets.has(input.targetType) && Object.hasOwn(value, 'sanitizationAttested'))
  )
    return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  if (value.links.length > value.maxFiles) return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  const links = value.links.map(link);
  const ids = new Set(links.map((item) => item.id));
  if (ids.size !== links.length) return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  for (let index = 1; index < links.length; index += 1) {
    const previous = links[index - 1]!;
    const current = links[index]!;
    if (
      current.createdAt < previous.createdAt ||
      (current.createdAt === previous.createdAt && current.id < previous.id)
    )
      return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  }
  return {
    ...(value as unknown as NativeAttachmentSetRow),
    links,
  };
}

function status(value: unknown, id: string): NativeAttachmentStatusRow {
  if (
    !exact(value, [
      'linkId',
      'setId',
      'sourceId',
      'sourceState',
      'linkState',
      'scan',
      'previewAvailable',
      'bindable',
      'expiresAt',
      'boundAt',
      'withdrawnAt',
    ]) ||
    value.linkId !== id ||
    !identifier(value.setId) ||
    !optionalIdentifier(value.sourceId) ||
    !(value.sourceState === null || oneOf(value.sourceState, sourceStates)) ||
    !oneOf(value.linkState, linkStates) ||
    !record(value.scan) ||
    !exact(value.scan, [
      'attempts',
      'lastResult',
      'lastError',
      'signature',
      'definitionUpdatedAt',
      'scannedAt',
    ]) ||
    !integer(value.scan.attempts) ||
    value.scan.attempts > 12 ||
    !(value.scan.lastResult === null || oneOf(value.scan.lastResult, scanResults)) ||
    !nullableText(value.scan.lastError, 160) ||
    !nullableText(value.scan.signature, 160) ||
    !optionalInteger(value.scan.definitionUpdatedAt) ||
    !optionalInteger(value.scan.scannedAt) ||
    !boolean(value.previewAvailable) ||
    !boolean(value.bindable) ||
    !integer(value.expiresAt) ||
    !optionalInteger(value.boundAt) ||
    !optionalInteger(value.withdrawnAt) ||
    (value.linkState === 'bound') !== (value.boundAt !== null) ||
    (value.linkState === 'withdrawn') !== (value.withdrawnAt !== null) ||
    (value.sourceId !== null && value.sourceState === null)
  )
    return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  return value as unknown as NativeAttachmentStatusRow;
}

function context(
  value: unknown,
  actor: NativeAttachmentPageContext,
  holdingSlug: string,
): Context {
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
      'scopeKind',
      'modules',
      'permissions',
      'expiresAt',
    ])
  )
    return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  const parsed = value as Context;
  const branchSlug = actor.scope.kind === 'holding' ? null : (actor.scope.branchSlug ?? null);
  const departmentSlug =
    actor.scope.kind === 'department' ? (actor.scope.departmentSlug ?? null) : null;
  if (
    actor.holdingSlug !== holdingSlug ||
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== branchSlug ||
    parsed.departmentSlug !== departmentSlug ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority.assignmentVersion ||
    parsed.revision !== actor.authority.revision ||
    parsed.role !== actor.role ||
    parsed.scopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    parsed.expiresAt <= Date.now() ||
    !Array.isArray(parsed.modules) ||
    parsed.modules.length !== actor.modules.length ||
    !parsed.modules.every((item) => actor.modules.includes(item)) ||
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length !== actor.iam.permissions.length ||
    !parsed.permissions.every((item) => actor.iam.permissions.includes(item))
  )
    return failure('NATIVE_ATTACHMENT_READ_CONTEXT_CHANGED');
  return parsed;
}

function configuration(raw: NativeAttachmentConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID');
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
    return failure('NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID');
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
      return failure('NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected || response.status !== 200) {
    let code = 'NATIVE_ATTACHMENT_READ_UNAVAILABLE';
    let message: string | undefined;
    if ([401, 403, 404, 409, 422].includes(response.status) && response.body) {
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
        // A malformed owner error keeps the bounded unavailable code.
      }
    } else {
      void response.body?.cancel().catch(() => undefined);
    }
    return failure(code, response.status, message);
  }
  if (
    response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json' ||
    !response.body
  )
    return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const part = await reader.read();
      if (part.done) break;
      size += part.value.length;
      if (size > 2 * 1024 * 1024) {
        await reader.cancel();
        return failure('NATIVE_ATTACHMENT_READ_RESPONSE_LIMIT');
      }
      chunks.push(part.value);
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)) as unknown;
  } catch {
    return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
  }
}

function outgoing(
  transport: NativeAttachmentReadTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeAttachmentPageContext,
  holdingSlug: string,
  path: string,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeAttachmentReadError('NATIVE_ATTACHMENT_READ_AUTHORITY_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeAttachmentReadError('NATIVE_ATTACHMENT_READ_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
    headers.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department' && actor.scope.departmentSlug)
    headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  const url = new URL(path, config.base);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  const combined = signal
    ? AbortSignal.any([signal, controller.signal])
    : controller.signal;
  return transport('GET', url, headers, undefined, combined).finally(() => clearTimeout(timer));
}

export function nativeAttachmentReadAvailable(): boolean {
  return process.env.ERPX_NATIVE_ATTACHMENT_READ_CONFIG !== undefined;
}

export function nativeAttachmentReadConfiguration(
  raw: string,
): NativeAttachmentConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeAttachmentReadError('NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID');
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
    return failure('NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID');
  return value as unknown as NativeAttachmentConfiguration;
}

export function nativeAttachmentReadFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeAttachmentReadError)) return undefined;
  if (error.code === 'NATIVE_ATTACHMENT_READ_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return { status: 403, code: error.code, message: error.message };
  if (error.status === 404)
    return { status: 404, code: error.code, message: error.message };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.code === 'NATIVE_ATTACHMENT_READ_REQUEST_INVALID')
    return {
      status: 422,
      code: 'ATTACHMENT_REQUEST_INVALID',
      message: 'Review the file request.',
    };
  if (
    error.status === 503 ||
    error.code === 'NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID' ||
    error.code === 'NATIVE_ATTACHMENT_READ_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_ATTACHMENT_READ_COOKIE_LIMIT' ||
    error.code === 'NATIVE_ATTACHMENT_READ_RESPONSE_INVALID' ||
    error.code === 'NATIVE_ATTACHMENT_READ_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_ATTACHMENT_READER_UNAVAILABLE',
      message: 'The native file reader is unavailable.',
    };
  return undefined;
}

export function createNativeAttachmentReadClient(
  raw: NativeAttachmentConfiguration,
  transport: NativeAttachmentReadTransport = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeAttachmentPageContext,
    holdingSlug: string,
    path: string,
    signal: AbortSignal | undefined,
    validate: (value: unknown) => unknown,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== holdingSlug)
      return failure('NATIVE_ATTACHMENT_READ_CONTEXT_CHANGED');
    const response = await outgoing(
      transport,
      config,
      incoming,
      actor,
      holdingSlug,
      path,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeAttachmentReadError) throw error;
      return failure('NATIVE_ATTACHMENT_READ_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
    context(value.context, actor, holdingSlug);
    return validate(value.data);
  };
  return {
    list: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeAttachmentPageContext,
      input: NativeAttachmentListInput,
      signal?: AbortSignal,
    ) => {
      if (
        !identifier(input.targetType) ||
        input.targetType.length > 80 ||
        !identifier(input.targetId) ||
        input.targetId.length > 180 ||
        (input.purposeKey !== undefined &&
          (!identifier(input.purposeKey) || input.purposeKey.length > 80)) ||
        (input.componentKind !== undefined &&
          !oneOf(input.componentKind, componentKinds)) ||
        (input.expectedTargetVersion !== undefined &&
          !integer(input.expectedTargetVersion, 1))
      )
        return Promise.reject(
          new NativeAttachmentReadError('NATIVE_ATTACHMENT_READ_REQUEST_INVALID'),
        );
      const search = new URLSearchParams({ targetType: input.targetType, targetId: input.targetId });
      if (input.purposeKey) search.set('purposeKey', input.purposeKey);
      if (input.componentKind) search.set('componentKind', input.componentKind);
      if (input.expectedTargetVersion !== undefined)
        search.set('expectedTargetVersion', String(input.expectedTargetVersion));
      if (input.readOnly) search.set('readOnly', 'true');
      return run(
        incoming,
        actor,
        input.holdingSlug,
        `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/attachment-sets?${search.toString()}`,
        signal,
        (value) => {
          if (!Array.isArray(value) || value.length > 100)
            return failure('NATIVE_ATTACHMENT_READ_RESPONSE_INVALID');
          return value.map((item) => set(item, input));
        },
      ) as Promise<NativeAttachmentSetRow[]>;
    },
    status: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeAttachmentPageContext,
      holdingSlug: string,
      linkId: string,
      signal?: AbortSignal,
    ) => {
      if (!identifier(linkId) || linkId.includes('/') || linkId.includes('\\'))
        return Promise.reject(
          new NativeAttachmentReadError('NATIVE_ATTACHMENT_READ_REQUEST_INVALID'),
        );
      return run(
        incoming,
        actor,
        holdingSlug,
        `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/attachment-links/${encodeURIComponent(linkId)}/status`,
        signal,
        (value) => status(value, linkId),
      ) as Promise<NativeAttachmentStatusRow>;
    },
  };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  _method: 'GET',
  url: URL,
  headers: Headers,
  _body: undefined,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = request(
      url,
      {
        method: 'GET',
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
    outgoing.end();
  });
}
