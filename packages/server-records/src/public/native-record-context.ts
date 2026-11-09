import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering facts from retained page authorization, never a grant. */
export type NativeRecordsPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  branchSlug?: string | null;
  departmentSlug?: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  modules: readonly string[];
  authority: { revision: number; assignmentVersion: number };
  iam: { permissions: readonly string[] };
};

export type NativeRecordContextConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativeRecordContextInput = Readonly<{
  holdingSlug: string;
  moduleId: string;
  recordId: string;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeRecordField = Readonly<{
  key: string;
  label: string;
  type: string;
  value: string | number | boolean;
  options?: string[];
}>;

export type NativeRecordView = Readonly<{
  moduleId: string;
  id: string;
  sectionId: string;
  title: string;
  description: string;
  status: string;
  fields: NativeRecordField[];
  version: number;
  createdBy: string;
  updatedAt: number;
}>;

export type NativeRecordReference = Readonly<{
  target_module: string;
  target_id: string;
  kind: string;
  ordinal: number;
  title: string;
  version: number;
  access: 'available' | 'denied';
}>;

export type NativeRecordBacklink = Readonly<{
  host_module: string;
  host_id: string;
  kind: string;
  title: string;
  version: number;
}>;

export type NativeRecordMention = Readonly<{
  target_kind: 'person' | 'group' | 'agent_pool';
  target_id: string;
  target_label: string;
  created_at: number;
}>;

export type NativeRecordSuggestion = Readonly<{
  id: string;
  field_key: string;
  proposed_value_json: string;
  source_versions_json: string;
  provenance_json: string;
  state: 'ready' | 'applied' | 'rejected' | 'conflicted';
  created_at: number;
}>;

export type NativeRecordHistory = Readonly<{
  from_status: string;
  to_status: string;
  action: 'submit' | 'approve' | 'reject' | 'complete' | 'reopen';
  note: string;
  actor_id: string;
  record_version: number;
  created_at: number;
}>;

export type NativeRecordContext = Readonly<{
  record: NativeRecordView;
  references: NativeRecordReference[];
  backlinks: NativeRecordBacklink[];
  mentions: NativeRecordMention[];
  suggestions: NativeRecordSuggestion[];
  history: NativeRecordHistory[];
}>;

export type NativeRecordContextTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeRecordContextError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Records request could not be completed.');
    this.name = 'NativeRecordContextError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeRecordContextError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const trimmed = (value: unknown, minimum: number, maximum: number): value is string =>
  text(value, maximum) &&
  [...value].length >= minimum &&
  value.trim() === value &&
  !/[\u0000-\u001f\u007f]/.test(value);
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;

const statuses = new Set(['draft', 'in_review', 'approved', 'completed', 'rejected']);
const edgeKinds = new Set(['dependsOn', 'blocks', 'billedBy', 'fulfills', 'authoredWith']);
const mentionKinds = new Set(['person', 'group', 'agent_pool']);
const suggestionStates = new Set(['ready', 'applied', 'rejected', 'conflicted']);
const actions = new Set(['submit', 'approve', 'reject', 'complete', 'reopen']);

const field = (value: unknown): value is NativeRecordField => {
  if (
    !record(value) ||
    !['key', 'label', 'type', 'value'].every((key) => Object.hasOwn(value, key)) ||
    Object.keys(value).some(
      (key) => !['key', 'label', 'type', 'value', 'options'].includes(key),
    )
  )
    return false;
  if (
    !trimmed(value.key, 1, 80) ||
    !trimmed(value.label, 1, 120) ||
    !trimmed(value.type, 1, 30) ||
    !(
      typeof value.value === 'string' ||
      typeof value.value === 'number' ||
      typeof value.value === 'boolean'
    )
  )
    return false;
  if (typeof value.value === 'number' && !Number.isFinite(value.value)) return false;
  if (value.options !== undefined) {
    if (
      !Array.isArray(value.options) ||
      value.options.length > 50 ||
      !value.options.every((option) => trimmed(option, 1, 120))
    )
      return false;
  }
  return true;
};

const recordView = (value: unknown, input: NativeRecordContextInput): value is NativeRecordView =>
  exact(value, [
    'moduleId',
    'id',
    'sectionId',
    'title',
    'description',
    'status',
    'fields',
    'version',
    'createdBy',
    'updatedAt',
  ]) &&
  value.moduleId === input.moduleId &&
  value.id === input.recordId &&
  trimmed(value.sectionId, 1, 128) &&
  trimmed(value.title, 1, 160) &&
  text(value.description, 2_000) &&
  (typeof value.description === 'string' && !value.description.includes('\u0000')) &&
  typeof value.status === 'string' &&
  statuses.has(value.status) &&
  Array.isArray(value.fields) &&
  value.fields.length >= 1 &&
  value.fields.length <= 64 &&
  value.fields.every(field) &&
  new Set((value.fields as NativeRecordField[]).map((item) => item.key)).size ===
    value.fields.length &&
  integer(value.version, 1) &&
  trimmed(value.createdBy, 1, 200) &&
  integer(value.updatedAt);

const reference = (value: unknown): value is NativeRecordReference =>
  exact(value, [
    'target_module',
    'target_id',
    'kind',
    'ordinal',
    'title',
    'version',
    'access',
  ]) &&
  trimmed(value.target_module, 1, 64) &&
  /^[a-z0-9_-]+$/.test(value.target_module as string) &&
  identifier(value.target_id) &&
  typeof value.kind === 'string' &&
  edgeKinds.has(value.kind) &&
  integer(value.ordinal) &&
  value.ordinal <= 49 &&
  trimmed(value.title, 1, 160) &&
  integer(value.version) &&
  (value.access === 'available' || value.access === 'denied') &&
  (value.access === 'available'
    ? integer(value.version, 1)
    : value.version === 0 && value.title === 'Unavailable record');

const backlink = (value: unknown): value is NativeRecordBacklink =>
  exact(value, ['host_module', 'host_id', 'kind', 'title', 'version']) &&
  trimmed(value.host_module, 1, 64) &&
  /^[a-z0-9_-]+$/.test(value.host_module as string) &&
  identifier(value.host_id) &&
  typeof value.kind === 'string' &&
  edgeKinds.has(value.kind) &&
  trimmed(value.title, 1, 160) &&
  integer(value.version, 1);

const mention = (value: unknown): value is NativeRecordMention =>
  exact(value, ['target_kind', 'target_id', 'target_label', 'created_at']) &&
  typeof value.target_kind === 'string' &&
  mentionKinds.has(value.target_kind) &&
  identifier(value.target_id) &&
  trimmed(value.target_label, 1, 160) &&
  integer(value.created_at);

const jsonString = (value: unknown, expected: (parsed: unknown) => boolean): value is string => {
  if (!text(value, 16 * 1024)) return false;
  try {
    return expected(JSON.parse(value as string));
  } catch {
    return false;
  }
};

const suggestion = (value: unknown): value is NativeRecordSuggestion =>
  exact(value, [
    'id',
    'field_key',
    'proposed_value_json',
    'source_versions_json',
    'provenance_json',
    'state',
    'created_at',
  ]) &&
  identifier(value.id) &&
  trimmed(value.field_key, 1, 80) &&
  jsonString(value.proposed_value_json, () => true) &&
  jsonString(value.source_versions_json, Array.isArray) &&
  jsonString(
    value.provenance_json,
    (parsed) => record(parsed),
  ) &&
  typeof value.state === 'string' &&
  suggestionStates.has(value.state) &&
  integer(value.created_at);

const history = (value: unknown): value is NativeRecordHistory =>
  exact(value, [
    'from_status',
    'to_status',
    'action',
    'note',
    'actor_id',
    'record_version',
    'created_at',
  ]) &&
  typeof value.from_status === 'string' &&
  statuses.has(value.from_status) &&
  typeof value.to_status === 'string' &&
  statuses.has(value.to_status) &&
  typeof value.action === 'string' &&
  actions.has(value.action) &&
  text(value.note, 1_000) &&
  (typeof value.note === 'string' && !value.note.includes('\u0000')) &&
  trimmed(value.actor_id, 1, 200) &&
  integer(value.record_version, 1) &&
  integer(value.created_at);

function contextData(value: unknown, input: NativeRecordContextInput): NativeRecordContext {
  if (
    !exact(value, [
      'record',
      'references',
      'backlinks',
      'mentions',
      'suggestions',
      'history',
    ]) ||
    !recordView(value.record, input) ||
    !Array.isArray(value.references) ||
    value.references.length > 50 ||
    !value.references.every(reference) ||
    !Array.isArray(value.backlinks) ||
    value.backlinks.length > 50 ||
    !value.backlinks.every(backlink) ||
    !Array.isArray(value.mentions) ||
    value.mentions.length > 200 ||
    !value.mentions.every(mention) ||
    !Array.isArray(value.suggestions) ||
    value.suggestions.length > 20 ||
    !value.suggestions.every(suggestion) ||
    !Array.isArray(value.history) ||
    value.history.length > 20 ||
    !value.history.every(history)
  )
    return failure('NATIVE_RECORD_CONTEXT_RESPONSE_INVALID');
  const targets = new Set<string>();
  for (const item of value.references as NativeRecordReference[]) {
    const key = `${item.target_module}/${item.target_id}`;
    if (targets.has(key)) return failure('NATIVE_RECORD_CONTEXT_RESPONSE_INVALID');
    targets.add(key);
  }
  const hosts = new Set<string>();
  for (const item of value.backlinks as NativeRecordBacklink[]) {
    const key = `${item.host_module}/${item.host_id}`;
    if (hosts.has(key)) return failure('NATIVE_RECORD_CONTEXT_RESPONSE_INVALID');
    hosts.add(key);
  }
  const ids = new Set<string>();
  for (const item of value.suggestions as NativeRecordSuggestion[]) {
    if (ids.has(item.id)) return failure('NATIVE_RECORD_CONTEXT_RESPONSE_INVALID');
    ids.add(item.id);
  }
  return value as unknown as NativeRecordContext;
}

function context(value: unknown, actor: NativeRecordsPageContext): Context {
  if (
    !exact(value, [
      'userId',
      'holdingId',
      'companyId',
      'branchId',
      'departmentId',
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
    return failure('NATIVE_RECORD_CONTEXT_RESPONSE_INVALID');
  const parsed = value as Context;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority.assignmentVersion ||
    parsed.revision !== actor.authority.revision ||
    parsed.role !== actor.role ||
    parsed.scopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    !Array.isArray(parsed.modules) ||
    parsed.modules.length > 128 ||
    !parsed.modules.every((item) => identifier(item)) ||
    parsed.modules.length !== actor.modules.length ||
    !parsed.modules.every((item) => actor.modules.includes(item)) ||
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length > 512 ||
    !parsed.permissions.every((item) => identifier(item)) ||
    parsed.permissions.length !== actor.iam.permissions.length ||
    !parsed.permissions.every((item) => actor.iam.permissions.includes(item))
  )
    return failure('NATIVE_RECORD_CONTEXT_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeRecordContextConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_RECORD_CONTEXT_CONFIGURATION_INVALID');
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
    raw.privateBaseUrl !== base.origin ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30_000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_RECORD_CONTEXT_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_RECORD_CONTEXT_CONFIGURATION_INVALID');
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
      return failure('NATIVE_RECORD_CONTEXT_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeRecordContextTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeRecordsPageContext,
  input: NativeRecordContextInput,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (
    !holding ||
    holding.holdingSlug !== input.holdingSlug ||
    holding.holdingId !== actor.holdingId
  )
    return Promise.reject(
      new NativeRecordContextError('NATIVE_RECORD_CONTEXT_AUTHORITY_INVALID'),
    );
  if (
    !/^[a-z0-9_-]{1,64}$/.test(input.moduleId) ||
    !identifier(input.recordId) ||
    input.recordId.length > 180 ||
    input.recordId.includes('/') ||
    input.recordId.includes('\\')
  )
    return Promise.reject(
      new NativeRecordContextError('NATIVE_RECORD_CONTEXT_REQUEST_INVALID'),
    );
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeRecordContextError('NATIVE_RECORD_CONTEXT_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/records/${encodeURIComponent(input.moduleId)}/${encodeURIComponent(input.recordId)}`,
    base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport('GET', url, headers, undefined, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
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
        return failure('NATIVE_RECORD_CONTEXT_RESPONSE_LIMIT');
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
    return failure('NATIVE_RECORD_CONTEXT_UNAVAILABLE', response.status);
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
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_RECORD_CONTEXT_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_RECORD_CONTEXT_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_RECORD_CONTEXT_RESPONSE_INVALID');
  }
}

export function nativeRecordContextAvailable(): boolean {
  return process.env.ERPX_NATIVE_RECORDS_CONFIG !== undefined;
}

export function nativeRecordContextConfiguration(
  raw: string,
): NativeRecordContextConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeRecordContextError('NATIVE_RECORD_CONTEXT_CONFIGURATION_INVALID');
  }
  if (
    !exact(value, ['privateBaseUrl', 'timeoutMs', 'holdings']) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings) ||
    !value.holdings.every(
      (holding: unknown) =>
        exact(holding, ['origin', 'holdingSlug', 'holdingId']) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_RECORD_CONTEXT_CONFIGURATION_INVALID');
  return value as unknown as NativeRecordContextConfiguration;
}

export function nativeRecordContextFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeRecordContextError)) return undefined;
  if (error.code === 'NATIVE_RECORD_CONTEXT_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_RECORD_CONTEXT_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: error.code === 'NATIVE_RECORD_CONTEXT_UNAVAILABLE' ? 'NOT_FOUND' : error.code,
      message: error.message,
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_RECORD_CONTEXT_UNAVAILABLE',
      message: 'The native Records reader is unavailable.',
    };
  if (error.code === 'NATIVE_RECORD_CONTEXT_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_RECORD_CONTEXT_UNAVAILABLE',
      message: 'The native Records reader is unavailable.',
    };
  if (error.code === 'NATIVE_RECORD_CONTEXT_REQUEST_INVALID')
    return {
      status: 422,
      code: 'RECORD_CONTEXT_REQUEST_INVALID',
      message: 'Choose a valid record in this workspace.',
    };
  if (
    error.code === 'NATIVE_RECORD_CONTEXT_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_RECORD_CONTEXT_COOKIE_LIMIT' ||
    error.code === 'NATIVE_RECORD_CONTEXT_RESPONSE_INVALID' ||
    error.code === 'NATIVE_RECORD_CONTEXT_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_RECORD_CONTEXT_UNAVAILABLE',
      message: 'The native Records reader is unavailable.',
    };
  return undefined;
}

export function createNativeRecordContextClient(
  raw: NativeRecordContextConfiguration,
  transport: NativeRecordContextTransport = directHttp,
) {
  const config = configuration(raw);
  return {
    context: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeRecordsPageContext,
      input: NativeRecordContextInput,
      signal?: AbortSignal,
    ): Promise<NativeRecordContext> => {
      if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
        return failure('NATIVE_RECORD_CONTEXT_CONTEXT_CHANGED');
      const response = await transportCall(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        input,
        config.timeout,
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeRecordContextError) throw error;
        return failure('NATIVE_RECORD_CONTEXT_UNAVAILABLE');
      });
      const value = await boundedJson(response);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_RECORD_CONTEXT_RESPONSE_INVALID');
      context(value.context, actor);
      return contextData(value.data, input);
    },
  };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
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
