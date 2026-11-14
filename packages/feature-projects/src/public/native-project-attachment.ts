import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import {
  NativeProjectsError,
  nativeProjectsConfiguration,
  type NativeProjectsConfiguration,
  type NativeProjectsPageContext,
} from './native-project-wbs';

export type NativeProjectAttachmentManifestItem = Readonly<{
  linkId: string;
  sourceId: string;
  purposeKey: string;
  checksumSha256: string;
  mediaType: string;
  bytes: number;
}>;

export type NativeProjectAttachmentBinding = Readonly<{
  setId: string;
  version: number;
  manifest: NativeProjectAttachmentManifestItem[];
}>;

export type NativeProjectAttachmentBindInput = Readonly<{
  key: string;
  expectedVersion: number;
}>;

export type NativeProjectAttachmentTransport = (
  method: 'POST' | 'PUT' | 'GET',
  url: URL,
  headers: Headers,
  body: Uint8Array,
  signal: AbortSignal,
) => Promise<Response>;

export type NativeProjectAttachmentLink = Readonly<{
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

export type NativeProjectAttachmentSet = Readonly<{
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
  links: NativeProjectAttachmentLink[];
}>;

export type NativeProjectAttachmentReservation = Readonly<{
  uploadIntentId: string;
  attachmentLinkId: string;
  uploadUrl: string;
  expiresAt: number;
}>;

export type NativeProjectAttachmentScan = Readonly<{
  attempts: number;
  lastResult: string | null;
  lastError: string | null;
  signature: string | null;
  definitionUpdatedAt: number | null;
  scannedAt: number | null;
}>;

export type NativeProjectAttachmentCompletion = Readonly<{
  uploadIntentId: string;
  attachmentLinkId: string;
  attachmentSetId: string;
  sourceId: string;
  state: string;
  status: string;
  scan: NativeProjectAttachmentScan;
  replay: boolean;
}>;

export type NativeProjectAttachmentUploadStatus = Readonly<{
  uploadIntentId: string;
  attachmentLinkId: string;
  attachmentSetId: string;
  sourceId: string | null;
  sourceState: string | null;
  linkState: string;
  status: string;
  scan: NativeProjectAttachmentScan;
  bindable: boolean;
  scannedAt: number | null;
  expiresAt: number;
}>;

export type NativeProjectAttachmentSetInput = Readonly<{
  key: string;
  targetId: string;
  purposeKey: string;
  componentRef?: string;
  expectedTargetVersion: number;
}>;

export type NativeProjectAttachmentReserveInput = Readonly<{
  key: string;
  filename: string;
  mediaType: string;
  bytes: number;
  caption?: string;
  supersedesLinkId?: string;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeProjectsError(code, status, message);
};
const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function settings(raw: string) {
  const value = nativeProjectsConfiguration(raw);
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin ||
    !integer(value.timeoutMs, 250) ||
    value.timeoutMs > 30000
  )
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  const holdings = new Map<
    string,
    { holdingId: string; holdingSlug: string; origin: string }
  >();
  for (const holding of value.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
      origin: holding.origin,
    });
  }
  return { base, holdings, timeout: value.timeoutMs };
}

function verifyContext(value: unknown, actor: NativeProjectsPageContext): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  const parsed = value as Context;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority?.assignmentVersion ||
    parsed.revision !== actor.authority?.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    parsed.expiresAt <= Date.now() ||
    !texts(parsed.modules) ||
    !sameSet(actor.modules, parsed.modules) ||
    !texts(parsed.permissions) ||
    !sameSet(actor.iam?.permissions ?? [], parsed.permissions)
  )
    return failure('NATIVE_PROJECTS_CONTEXT_CHANGED');
  return parsed;
}

function bindingData(
  value: unknown,
  setId: string,
  expectedVersion: number,
): NativeProjectAttachmentBinding {
  if (
    !exact(value, [
      'id', 'version', 'status', 'ownerVersion', 'attachmentSetId',
      'attachmentLinkId', 'manifest', 'setId',
    ])
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  if (
    !identifier(value.id) ||
    !integer(value.version, 1) ||
    value.status !== 'bound' ||
    !integer(value.ownerVersion, 1) ||
    value.attachmentSetId !== setId ||
    value.setId !== setId ||
    value.version !== expectedVersion + 1 ||
    value.ownerVersion !== expectedVersion + 1 ||
    !identifier(value.attachmentLinkId) ||
    !Array.isArray(value.manifest) ||
    value.manifest.length < 1 ||
    value.manifest.length > 100
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  const seen = new Set<string>();
  for (const item of value.manifest) {
    if (
      !exact(item, [
        'linkId', 'sourceId', 'purposeKey', 'checksumSha256', 'mediaType', 'bytes',
      ]) ||
      !identifier(item.linkId) ||
      !identifier(item.sourceId) ||
      !text(item.purposeKey) ||
      !/^[a-f0-9]{64}$/.test(String(item.checksumSha256)) ||
      !text(item.mediaType) ||
      !integer(item.bytes, 1) ||
      item.bytes > 10 * 1024 * 1024 ||
      seen.has(item.linkId)
    )
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    seen.add(item.linkId);
  }
  const manifest = value.manifest as NativeProjectAttachmentManifestItem[];
  if (manifest[0]!.linkId !== value.attachmentLinkId)
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  return { setId, version: value.ownerVersion, manifest };
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
        return failure('NATIVE_PROJECTS_RESPONSE_LIMIT');
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

async function boundedReply(response: Response, expected = 200): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PROJECTS_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== expected) {
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
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_PROJECTS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  }
}

function scanData(value: unknown): NativeProjectAttachmentScan {
  if (
    !exact(value, [
      'attempts', 'lastResult', 'lastError', 'signature', 'definitionUpdatedAt', 'scannedAt',
    ]) ||
    !integer(value.attempts) ||
    (value.lastResult !== null && !text(value.lastResult)) ||
    (value.lastError !== null && !text(value.lastError)) ||
    (value.signature !== null && !text(value.signature)) ||
    (value.definitionUpdatedAt !== null && !integer(value.definitionUpdatedAt, 1)) ||
    (value.scannedAt !== null && !integer(value.scannedAt, 1))
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  return value as NativeProjectAttachmentScan;
}

function setData(value: unknown): NativeProjectAttachmentSet {
  if (
    !exact(value, [
      'id', 'targetType', 'targetId', 'componentKind', 'componentRef', 'purposeKey',
      'expectedTargetVersion', 'policyVersion', 'classification', 'requiredCount',
      'maxFiles', 'state', 'createdAt', 'updatedAt', 'links',
    ]) ||
    !identifier(value.id) ||
    value.targetType !== 'project' ||
    !identifier(value.targetId) ||
    value.componentKind !== 'record' ||
    (value.componentRef !== null && !text(value.componentRef)) ||
    !text(value.purposeKey) ||
    !integer(value.expectedTargetVersion, 1) ||
    !integer(value.policyVersion, 1) ||
    !text(value.classification) ||
    !integer(value.requiredCount) ||
    !integer(value.maxFiles, 1) ||
    !text(value.state) ||
    !integer(value.createdAt) ||
    !integer(value.updatedAt) ||
    !Array.isArray(value.links) ||
    value.links.length > 100
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  const seen = new Set<string>();
  for (const link of value.links) {
    if (
      !exact(link, [
        'id', 'sourceId', 'filename', 'mediaType', 'bytes', 'sourceState', 'linkState',
        'caption', 'previewAvailable', 'createdAt', 'expiresAt', 'boundAt',
      ]) ||
      !identifier(link.id) ||
      (link.sourceId !== null && !identifier(link.sourceId)) ||
      !text(link.filename) ||
      !text(link.mediaType) ||
      (link.bytes !== null && !integer(link.bytes, 1)) ||
      (link.sourceState !== null && !text(link.sourceState)) ||
      !text(link.linkState) ||
      (link.caption !== null && !text(link.caption)) ||
      typeof link.previewAvailable !== 'boolean' ||
      !integer(link.createdAt) ||
      !integer(link.expiresAt) ||
      (link.boundAt !== null && !integer(link.boundAt)) ||
      seen.has(link.id)
    )
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    seen.add(link.id);
  }
  return value as NativeProjectAttachmentSet;
}

function reservationData(
  value: unknown,
  holdingSlug: string,
): NativeProjectAttachmentReservation {
  if (
    !exact(value, ['uploadIntentId', 'attachmentLinkId', 'uploadUrl', 'expiresAt']) ||
    !identifier(value.uploadIntentId) ||
    !identifier(value.attachmentLinkId) ||
    !text(value.uploadUrl, 2048) ||
    value.uploadUrl !==
      `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/project-attachments/uploads/${encodeURIComponent(value.uploadIntentId)}/content` ||
    !integer(value.expiresAt, 1)
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  return value as NativeProjectAttachmentReservation;
}

function completionData(
  value: unknown,
  uploadId: string,
  setId: string,
): NativeProjectAttachmentCompletion {
  if (
    !exact(value, [
      'uploadIntentId', 'attachmentLinkId', 'attachmentSetId', 'sourceId', 'state',
      'status', 'scan', 'replay',
    ]) ||
    value.uploadIntentId !== uploadId ||
    value.attachmentSetId !== setId ||
    !identifier(value.attachmentLinkId) ||
    !identifier(value.sourceId) ||
    !text(value.state) ||
    !text(value.status) ||
    typeof value.replay !== 'boolean'
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  scanData(value.scan);
  return value as NativeProjectAttachmentCompletion;
}

function uploadStatusData(
  value: unknown,
  uploadId: string,
): NativeProjectAttachmentUploadStatus {
  if (
    !exact(value, [
      'uploadIntentId', 'attachmentLinkId', 'attachmentSetId', 'sourceId', 'sourceState',
      'linkState', 'status', 'scan', 'bindable', 'scannedAt', 'expiresAt',
    ]) ||
    value.uploadIntentId !== uploadId ||
    !identifier(value.attachmentLinkId) ||
    !identifier(value.attachmentSetId) ||
    (value.sourceId !== null && !identifier(value.sourceId)) ||
    (value.sourceState !== null && !text(value.sourceState)) ||
    !text(value.linkState) ||
    !text(value.status) ||
    typeof value.bindable !== 'boolean' ||
    (value.scannedAt !== null && !integer(value.scannedAt, 1)) ||
    !integer(value.expiresAt, 1)
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  scanData(value.scan);
  return value as NativeProjectAttachmentUploadStatus;
}

function intakeCall(
  transport: NativeProjectAttachmentTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string; origin: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  method: 'POST' | 'PUT' | 'GET',
  path: string,
  body: Uint8Array,
  timeout: number,
  key?: string,
  contentType?: string,
): Promise<Response> {
  if (!actor.authority || !actor.iam)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID'));
  const captured = {
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  } as const;
  if (Object.entries(captured).some(([name, value]) => incoming.get(name) !== value))
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
    ...captured,
  });
  if (key !== undefined) {
    if (key.length < 8 || key.length > 180 || !/^[a-zA-Z0-9:_.-]+$/.test(key))
      return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
    headers.set('idempotency-key', key);
    const origin = incoming.get('origin');
    if (origin !== holding.origin)
      return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID'));
    headers.set('origin', origin);
  }
  if (contentType !== undefined) headers.set('content-type', contentType);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
    headers.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department')
    headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}${path}`,
    base,
  );
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  return transport(method, url, headers, body, controller.signal)
    .finally(() => clearTimeout(timer))
    .catch((error: unknown) => {
      if (error instanceof NativeProjectsError) throw error;
      return failure('NATIVE_PROJECTS_UNAVAILABLE');
    });
}
function requestCall(
  transport: NativeProjectAttachmentTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string; origin: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  setId: string,
  input: NativeProjectAttachmentBindInput,
  timeout: number,
): Promise<Response> {
  if (!actor.authority || !actor.iam)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID'));
  if (!identifier(setId) || setId.includes('/') || setId.includes('\\'))
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  if (!identifier(input.key) || input.key.length < 8 || input.key.length > 180 || !/^[a-zA-Z0-9:_.-]+$/.test(input.key))
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  if (!integer(input.expectedVersion, 1))
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  const captured = {
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  } as const;
  if (
    Object.entries(captured).some(
      ([name, value]) => incoming.get(name) !== value,
    )
  )
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
  const origin = incoming.get('origin');
  if (origin !== holding.origin)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    origin,
    'cache-control': 'no-cache',
    'content-type': 'application/json',
    'idempotency-key': input.key,
    ...captured,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
    headers.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department')
    headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  const body = new TextEncoder().encode(
    JSON.stringify({ expectedTargetVersion: input.expectedVersion }),
  );
  if (body.byteLength > 1024)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/project-attachments/${encodeURIComponent(setId)}/bind`,
    base,
  );
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  return transport('POST', url, headers, body, controller.signal)
    .finally(() => clearTimeout(timer))
    .catch((error: unknown) => {
      if (error instanceof NativeProjectsError) throw error;
      return failure('NATIVE_PROJECTS_UNAVAILABLE');
    });
}

export function createNativeProjectAttachmentClient(
  raw: NativeProjectsConfiguration,
  transport: NativeProjectAttachmentTransport = directHttpPost,
) {
  const config = settings(JSON.stringify(raw));
  const readData = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProjectsPageContext,
    method: 'POST' | 'PUT' | 'GET',
    path: string,
    body: Uint8Array,
    expected: number,
    options: { key?: string; contentType?: string } = {},
  ): Promise<unknown> => {
    const response = await intakeCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      actor,
      method,
      path,
      body,
      config.timeout,
      options.key,
      options.contentType,
    );
    const value = await boundedReply(response, expected);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    verifyContext(value.context, actor);
    return value.data;
  };
  const jsonBody = (value: unknown): Uint8Array =>
    new TextEncoder().encode(JSON.stringify(value));
  const reservationPath = (
    actor: NativeProjectsPageContext,
    setId: string,
    uploadId: string,
  ) =>
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/attachment-sets/${encodeURIComponent(setId)}/uploads/${encodeURIComponent(uploadId)}/content`;
  return {
    async bind(
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      setId: string,
      input: NativeProjectAttachmentBindInput,
    ): Promise<NativeProjectAttachmentBinding> {
      const response = await requestCall(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        setId,
        input,
        config.timeout,
      );
      const value = await boundedReply(response);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
      verifyContext(value.context, actor);
      return bindingData(value.data, setId, input.expectedVersion);
    },
    async createSet(
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      input: NativeProjectAttachmentSetInput,
    ): Promise<NativeProjectAttachmentSet> {
      if (!identifier(input.targetId) || !text(input.purposeKey))
        return failure('NATIVE_PROJECTS_REQUEST_INVALID');
      const body = jsonBody({
        targetType: 'project',
        targetId: input.targetId,
        componentKind: 'record',
        ...(input.componentRef ? { componentRef: input.componentRef } : {}),
        purposeKey: input.purposeKey,
        expectedTargetVersion: input.expectedTargetVersion,
      });
      if (body.byteLength > 8 * 1024)
        return failure('NATIVE_PROJECTS_REQUEST_INVALID');
      return setData(
        await readData(
          incoming,
          actor,
          'POST',
          '/project-attachments/attachment-sets',
          body,
          201,
          { key: input.key, contentType: 'application/json' },
        ),
      );
    },
    async reserveFile(
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      setId: string,
      input: NativeProjectAttachmentReserveInput,
    ): Promise<NativeProjectAttachmentReservation> {
      if (!identifier(setId) || setId.includes('/') || setId.includes('\\'))
        return failure('NATIVE_PROJECTS_REQUEST_INVALID');
      const body = jsonBody({
        filename: input.filename,
        mediaType: input.mediaType,
        bytes: input.bytes,
        ...(input.caption ? { caption: input.caption } : {}),
        ...(input.supersedesLinkId ? { supersedesLinkId: input.supersedesLinkId } : {}),
      });
      if (body.byteLength > 64 * 1024)
        return failure('NATIVE_PROJECTS_REQUEST_INVALID');
      const data = reservationData(
        await readData(
          incoming,
          actor,
          'POST',
          `/project-attachments/attachment-sets/${encodeURIComponent(setId)}/files`,
          body,
          201,
          { key: input.key, contentType: 'application/json' },
        ),
        actor.holdingSlug,
      );
      return { ...data, uploadUrl: reservationPath(actor, setId, data.uploadIntentId) };
    },
    async completeUpload(
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      setId: string,
      uploadId: string,
      contentType: string,
      bytes: Uint8Array,
    ): Promise<NativeProjectAttachmentCompletion> {
      if (
        !identifier(setId) ||
        !identifier(uploadId) ||
        !text(contentType) ||
        contentType.length === 0 ||
        contentType.length > 100 ||
        bytes.byteLength < 1 ||
        bytes.byteLength > 10 * 1024 * 1024
      )
        return failure('NATIVE_PROJECTS_REQUEST_INVALID');
      return completionData(
        await readData(
          incoming,
          actor,
          'PUT',
          `/project-attachments/uploads/${encodeURIComponent(uploadId)}/content`,
          bytes,
          202,
          { contentType },
        ),
        uploadId,
        setId,
      );
    },
    async uploadStatus(
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      uploadId: string,
    ): Promise<NativeProjectAttachmentUploadStatus> {
      if (!identifier(uploadId))
        return failure('NATIVE_PROJECTS_REQUEST_INVALID');
      return uploadStatusData(
        await readData(
          incoming,
          actor,
          'GET',
          `/project-attachments/uploads/${encodeURIComponent(uploadId)}/status`,
          new Uint8Array(),
          200,
        ),
        uploadId,
      );
    },
  };
}

/** Route switch entry. Once the Projects configuration is present a selected
 * project set never falls back to the retained generic binder. */
export async function nativeProjectAttachmentBind(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  setId: string,
  input: NativeProjectAttachmentBindInput,
): Promise<NativeProjectAttachmentBinding> {
  const raw = process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  if (raw === undefined)
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID', 503);
  let parsed: NativeProjectsConfiguration;
  try {
    parsed = nativeProjectsConfiguration(raw);
  } catch {
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID', 503);
  }
  return createNativeProjectAttachmentClient(parsed).bind(incoming, actor, setId, input);
}

/** Bounded configuration entry for a selected project intake operation. */
function configuredClient(raw: string | undefined) {
  if (raw === undefined)
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID', 503);
  let parsed: NativeProjectsConfiguration;
  try {
    parsed = nativeProjectsConfiguration(raw);
  } catch {
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID', 503);
  }
  return createNativeProjectAttachmentClient(parsed);
}

export async function nativeProjectAttachmentCreateSet(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  input: NativeProjectAttachmentSetInput,
): Promise<NativeProjectAttachmentSet> {
  return configuredClient(process.env.ERPX_NATIVE_PROJECTS_CONFIG).createSet(
    incoming,
    actor,
    input,
  );
}

export async function nativeProjectAttachmentReserveFile(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  setId: string,
  input: NativeProjectAttachmentReserveInput,
): Promise<NativeProjectAttachmentReservation> {
  return configuredClient(process.env.ERPX_NATIVE_PROJECTS_CONFIG).reserveFile(
    incoming,
    actor,
    setId,
    input,
  );
}

export async function nativeProjectAttachmentCompleteUpload(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  setId: string,
  uploadId: string,
  contentType: string,
  bytes: Uint8Array,
): Promise<NativeProjectAttachmentCompletion> {
  return configuredClient(process.env.ERPX_NATIVE_PROJECTS_CONFIG).completeUpload(
    incoming,
    actor,
    setId,
    uploadId,
    contentType,
    bytes,
  );
}

export async function nativeProjectAttachmentUploadStatus(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  uploadId: string,
): Promise<NativeProjectAttachmentUploadStatus> {
  return configuredClient(process.env.ERPX_NATIVE_PROJECTS_CONFIG).uploadStatus(
    incoming,
    actor,
    uploadId,
  );
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttpPost(
  method: 'POST' | 'PUT' | 'GET',
  url: URL,
  headers: Headers,
  body: Uint8Array,
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
