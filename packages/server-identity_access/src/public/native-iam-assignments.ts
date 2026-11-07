import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering and command input from retained page authorization, never a grant. */
export type NativeIamPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  authority: { revision: number; assignmentVersion: number };
  iam: { permissions: readonly string[] };
};

export type NativeIamAssignmentsConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { permissions: string[] };

export type NativeIamAssignmentsReadInput = Readonly<{
  holdingSlug: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeIamAssignmentRow = Readonly<{
  id: string;
  user_id: string;
  role: string;
  modules_json: string;
  company_id: string;
  branch_id: string | null;
  valid_from: number;
  valid_until: number;
  revoked_at: number | null;
  version: number;
  scope_kind: string;
  scope_node_id: string | null;
  parent_id: string | null;
  source: string;
  source_id: string | null;
  identity_status: string | null;
  login_policy: string | null;
  person_id: string | null;
  connection_id: string | null;
  identity_version: number | null;
  group_name: string | null;
  mapping_connection_id: string | null;
  directory_enabled: boolean | null;
  observed_at: number | null;
  connection_status: string | null;
  active: boolean;
}>;

export type NativeIamRoleRow = Readonly<{
  id: string;
  name: string;
  companyId: string;
  revision: number;
  builtIn: boolean;
  permissions: string[];
  grantablePermissions: string[];
  scopes: string[];
  status: string;
}>;

export type NativeIamAssignmentsWorkspace = Readonly<{
  assignments: NativeIamAssignmentRow[];
  roles: NativeIamRoleRow[];
  evaluatedAt: number;
  currentAssignmentId: string;
  currentUserId: string;
  scopeKind: string;
  scopeNodeId: string | null;
}>;

export type NativeIamAssignmentReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
}>;

export type NativeIamAssignmentsTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeIamAssignmentsError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Identity Access request could not be completed.');
    this.name = 'NativeIamAssignmentsError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeIamAssignmentsError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const optionalIdentifier = (value: unknown): value is string | null =>
  value === null || identifier(value);
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const optionalBoolean = (value: unknown): value is boolean | null =>
  value === null || boolean(value);
const optionalInteger = (value: unknown): value is number | null =>
  value === null || integer(value);
const scopeKinds = new Set(['holding', 'branch', 'department']);
const sources = new Set([
  'legacy',
  'manual',
  'directory',
  'emergency',
  'policy',
  'lifecycle',
  'provisioning',
]);
const identityStatuses = new Set(['active', 'suspended', 'superseded']);
const loginPolicies = new Set(['local', 'directory', 'emergency']);

const assignment = (value: unknown): value is NativeIamAssignmentRow =>
  exact(value, [
    'id',
    'user_id',
    'role',
    'modules_json',
    'company_id',
    'branch_id',
    'valid_from',
    'valid_until',
    'revoked_at',
    'version',
    'scope_kind',
    'scope_node_id',
    'parent_id',
    'source',
    'source_id',
    'identity_status',
    'login_policy',
    'person_id',
    'connection_id',
    'identity_version',
    'group_name',
    'mapping_connection_id',
    'directory_enabled',
    'observed_at',
    'connection_status',
    'active',
  ]) &&
  identifier(value.id) &&
  identifier(value.user_id) &&
  trimmed(value.role, 1, 200) &&
  text(value.modules_json) &&
  Array.isArray(JSON.parse(value.modules_json as string)) &&
  identifier(value.company_id) &&
  optionalIdentifier(value.branch_id) &&
  integer(value.valid_from) &&
  integer(value.valid_until) &&
  value.valid_until > value.valid_from &&
  optionalInteger(value.revoked_at) &&
  integer(value.version, 1) &&
  value.version <= 1_000_000_000 &&
  typeof value.scope_kind === 'string' &&
  scopeKinds.has(value.scope_kind) &&
  optionalIdentifier(value.scope_node_id) &&
  optionalIdentifier(value.parent_id) &&
  typeof value.source === 'string' &&
  sources.has(value.source) &&
  optionalIdentifier(value.source_id) &&
  (value.identity_status === null ||
    (typeof value.identity_status === 'string' &&
      identityStatuses.has(value.identity_status))) &&
  (value.login_policy === null ||
    (typeof value.login_policy === 'string' && loginPolicies.has(value.login_policy))) &&
  optionalIdentifier(value.person_id) &&
  optionalIdentifier(value.connection_id) &&
  optionalInteger(value.identity_version) &&
  optionalIdentifier(value.group_name) &&
  optionalIdentifier(value.mapping_connection_id) &&
  optionalBoolean(value.directory_enabled) &&
  optionalInteger(value.observed_at) &&
  optionalIdentifier(value.connection_status) &&
  boolean(value.active);

const role = (value: unknown): value is NativeIamRoleRow =>
  exact(value, [
    'id',
    'name',
    'companyId',
    'revision',
    'builtIn',
    'permissions',
    'grantablePermissions',
    'scopes',
    'status',
  ]) &&
  identifier(value.id) &&
  trimmed(value.name, 1, 300) &&
  identifier(value.companyId) &&
  integer(value.revision, 1) &&
  boolean(value.builtIn) &&
  Array.isArray(value.permissions) &&
  value.permissions.length <= 512 &&
  value.permissions.every((item) => identifier(item)) &&
  Array.isArray(value.grantablePermissions) &&
  value.grantablePermissions.length <= 512 &&
  value.grantablePermissions.every((item) => identifier(item)) &&
  Array.isArray(value.scopes) &&
  value.scopes.length <= 4 &&
  value.scopes.every(
    (item) => typeof item === 'string' && scopeKinds.has(item),
  ) &&
  (value.status === 'active' || value.status === 'archived');

const trimmed = (value: unknown, minimum: number, maximum: number): value is string =>
  text(value, maximum) &&
  [...value].length >= minimum &&
  value.trim() === value &&
  !/[\u0000-\u001f\u007f]/.test(value);

function workspace(
  value: unknown,
  actor: NativeIamPageContext,
): NativeIamAssignmentsWorkspace {
  if (
    !exact(value, [
      'assignments',
      'roles',
      'evaluatedAt',
      'currentAssignmentId',
      'currentUserId',
      'scopeKind',
      'scopeNodeId',
    ]) ||
    !Array.isArray(value.assignments) ||
    value.assignments.length > 1_001 ||
    !value.assignments.every(assignment) ||
    !Array.isArray(value.roles) ||
    value.roles.length > 1_001 ||
    !value.roles.every(role) ||
    !integer(value.evaluatedAt) ||
    value.currentAssignmentId !== actor.assignmentId ||
    value.currentUserId !== actor.userId ||
    value.scopeKind !== actor.assignmentScope ||
    optionalIdentifier(value.scopeNodeId) === false ||
    value.scopeNodeId !==
      (actor.assignmentScope === 'holding'
        ? null
        : actor.assignmentScope === 'branch'
          ? actor.branchId
          : actor.departmentId)
  )
    return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
  const assignmentIds = new Set<string>();
  const roleIds = new Set<string>();
  for (const item of value.assignments as NativeIamAssignmentRow[]) {
    if (
      item.company_id !== actor.companyId ||
      assignmentIds.has(item.id)
    )
      return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
    if (
      actor.assignmentScope !== 'holding' &&
      item.scope_node_id !==
        (actor.assignmentScope === 'branch' ? actor.branchId : actor.departmentId)
    )
      return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
    assignmentIds.add(item.id);
  }
  for (const item of value.roles as NativeIamRoleRow[]) {
    if (item.companyId !== '*' && item.companyId !== actor.companyId)
      return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
    if (roleIds.has(item.id))
      return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
    roleIds.add(item.id);
  }
  return value as unknown as NativeIamAssignmentsWorkspace;
}

function receipt(
  value: unknown,
  id: string,
  expectedVersion: number,
): NativeIamAssignmentReceipt {
  if (
    !exact(value, ['id', 'status', 'version']) ||
    !identifier(value.id) ||
    value.id !== id ||
    value.status !== 'revoked' ||
    !integer(value.version, 2) ||
    value.version !== expectedVersion + 1
  )
    return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
  return value as unknown as NativeIamAssignmentReceipt;
}

function context(
  value: unknown,
  actor: NativeIamPageContext,
  input: NativeIamAssignmentsReadInput,
): Context {
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
      'permissions',
      'expiresAt',
    ])
  )
    return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
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
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length > 512 ||
    !parsed.permissions.every((item) => identifier(item)) ||
    parsed.permissions.length !== actor.iam.permissions.length ||
    !parsed.permissions.every((item) => actor.iam.permissions.includes(item))
  )
    return failure('NATIVE_IAM_ASSIGNMENTS_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_IAM_ASSIGNMENTS_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeIamAssignmentsConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID');
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
    return failure('NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID');
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
      return failure('NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeIamAssignmentsTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeIamPageContext,
  input: NativeIamAssignmentsReadInput,
  method: 'GET' | 'POST',
  path: string,
  body: string | undefined,
  key: string | undefined,
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
      new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_AUTHORITY_INVALID'),
    );
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(
        new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_COOKIE_LIMIT'),
      );
    headers.set('cookie', cookie);
  }
  if (input.expected) {
    if (
      !identifier(input.expected.companyId) ||
      !identifier(input.expected.assignmentId) ||
      !integer(input.expected.revision) ||
      !integer(input.expected.assignmentVersion, 1)
    )
      return Promise.reject(
        new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_REQUEST_INVALID'),
      );
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set(
      'x-erpx-context-assignment-version',
      String(input.expected.assignmentVersion),
    );
  }
  if (method === 'POST') {
    if (!key || !text(key, 180) || key.length < 8 || !/^[A-Za-z0-9:_.-]+$/.test(key))
      return Promise.reject(
        new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_REQUEST_INVALID'),
      );
    if (body === undefined)
      return Promise.reject(
        new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_REQUEST_INVALID'),
      );
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', key);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/iam/assignments${path}`,
    base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport(method, url, headers, body, controller.signal).finally(() => {
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
        return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_LIMIT');
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
    return failure('NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE', response.status);
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
    return failure(
      code ?? 'NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE',
      response.status,
      message,
    );
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
  }
}

function expectedVersion(body: string) {
  try {
    const value: unknown = JSON.parse(body);
    if (record(value) && integer(value.expectedVersion, 1)) return value.expectedVersion;
  } catch {
    // Fall through to the bounded request failure.
  }
  return failure('NATIVE_IAM_ASSIGNMENTS_REQUEST_INVALID');
}

export function nativeIamAssignmentsAvailable(): boolean {
  return process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG !== undefined;
}

export function nativeIamAssignmentsConfiguration(
  raw: string,
): NativeIamAssignmentsConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID');
  return value as unknown as NativeIamAssignmentsConfiguration;
}

export function nativeIamAssignmentsFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeIamAssignmentsError)) return undefined;
  if (error.code === 'NATIVE_IAM_ASSIGNMENTS_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: error.code === 'NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE' ? 'NOT_FOUND' : error.code,
      message: error.message,
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE',
      message: 'The native Identity Access service is unavailable.',
    };
  if (error.code === 'NATIVE_IAM_ASSIGNMENTS_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE',
      message: 'The native Identity Access reader is unavailable.',
    };
  if (error.code === 'NATIVE_IAM_ASSIGNMENTS_REQUEST_INVALID')
    return {
      status: 422,
      code: 'IAM_ASSIGNMENT_REQUEST_INVALID',
      message: 'Review the assignment request.',
    };
  if (
    error.code === 'NATIVE_IAM_ASSIGNMENTS_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_IAM_ASSIGNMENTS_COOKIE_LIMIT' ||
    error.code === 'NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID' ||
    error.code === 'NATIVE_IAM_ASSIGNMENTS_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE',
      message: 'The native Identity Access reader is unavailable.',
    };
  return undefined;
}

export function createNativeIamAssignmentsClient(
  raw: NativeIamAssignmentsConfiguration,
  transport: NativeIamAssignmentsTransport = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeIamPageContext,
    input: NativeIamAssignmentsReadInput,
    method: 'GET' | 'POST',
    path: string,
    body: string | undefined,
    key: string | undefined,
    signal: AbortSignal | undefined,
    validate: (value: unknown) => unknown,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_IAM_ASSIGNMENTS_CONTEXT_CHANGED');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      actor,
      input,
      method,
      path,
      body,
      key,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeIamAssignmentsError) throw error;
      return failure('NATIVE_IAM_ASSIGNMENTS_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_IAM_ASSIGNMENTS_RESPONSE_INVALID');
    context(value.context, actor, input);
    return validate(value.data);
  };
  return {
    workspace: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeIamPageContext,
      input: NativeIamAssignmentsReadInput,
      signal?: AbortSignal,
    ) =>
      run(
        incoming,
        actor,
        input,
        'GET',
        '',
        undefined,
        undefined,
        signal,
        (value) => workspace(value, actor),
      ) as Promise<NativeIamAssignmentsWorkspace>,
    revoke: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeIamPageContext,
      id: string,
      body: string,
      key: string,
      input: NativeIamAssignmentsReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(
          new NativeIamAssignmentsError('NATIVE_IAM_ASSIGNMENTS_REQUEST_INVALID'),
        );
      const expected = expectedVersion(body);
      return run(
        incoming,
        actor,
        input,
        'POST',
        `/${encodeURIComponent(id)}/revoke`,
        body,
        key,
        signal,
        (value) => receipt(value, id, expected),
      ) as Promise<NativeIamAssignmentReceipt>;
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
