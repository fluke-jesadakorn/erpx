import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeCustomerAccessPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  scope:
    | { kind: 'holding' }
    | { kind: 'branch'; branchSlug: string }
    | { kind: 'department'; branchSlug?: string; departmentSlug: string };
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type NativeCustomerAccessConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeCustomerAccessReadInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeCustomerAccessRow = Readonly<{
  id: string;
  company_id: string;
  order_id: string;
  customer_id: string;
  email: string;
  expires_at: number;
  status: string;
  reason: string;
  created_by: string;
  approved_by: string | null;
  reviewed_at: number | null;
  version: number;
  created_at: number;
  updated_at: number;
  number: string;
  order_state: string;
  branch_id: string | null;
  department_id: string | null;
  reviewers: number;
  canReview: boolean;
  canReject: boolean;
  canRevoke: boolean;
  canInvite: boolean;
}>;

export type NativeCustomerAccessSource = Readonly<{
  id: string;
  number: string;
  name: string;
  version: number;
}>;

export type NativeCustomerAccessWorkspace = Readonly<{
  rows: NativeCustomerAccessRow[];
  sources: NativeCustomerAccessSource[];
}>;

export type NativeCustomerAccessEvent = Readonly<{
  version: number;
  action: string;
  actor_id: string;
  reason: string;
  created_at: number;
}>;

export type NativeCustomerAccessRead = Readonly<{
  binding_version: number;
  user_id: string;
  read_at: number;
  projection_version: string;
}>;

export type NativeCustomerAccessDetail = Readonly<{
  binding: NativeCustomerAccessRow;
  events: NativeCustomerAccessEvent[];
  reads: NativeCustomerAccessRead[];
}>;

export type NativeCustomerAccessReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
}>;

export type NativePortalInvitationReceipt = Readonly<{
  id: string;
  bindingId: string;
  invitationId: string;
  generation: number;
  version: number;
  state: 'queued';
  expiresAt: number;
}>;

export type NativePortalInvitationRow = Readonly<{
  id: string;
  binding_id: string;
  generation: number;
  holding_slug: string;
  email: string;
  state: string;
  token_expires_at: number;
  attempts: number;
  next_attempt_at: number;
  last_error: string | null;
  created_by: string;
  created_at: number;
  sent_at: number | null;
  accepted_at: number | null;
  accepted_user_id: string | null;
}>;

export type NativePortalInvitationEvent = Readonly<{
  id: string;
  invitation_id: string;
  action: string;
  actor_id: string;
  reason: string;
  created_at: number;
}>;

export type NativePortalInvitationHistory = Readonly<{
  invitations: NativePortalInvitationRow[];
  invitationEvents: NativePortalInvitationEvent[];
}>;

export type NativeCustomerAccessCommandInput = Readonly<
  | {
      orderId: string;
      sourceVersion: number;
      email: string;
      expiresAt: number;
      reason: string;
    }
  | { action: 'approve' | 'reject' | 'revoke'; expectedVersion: number; reason: string }
>;

export type NativeCustomerAccessTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeCustomerAccessError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native customer access request could not be completed.');
    this.name = 'NativeCustomerAccessError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeCustomerAccessError(code, status, message);
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
const optionalIdentifier = (value: unknown) =>
  value === null || (identifier(value) && !/[\u0000-\u001f\u007f]/.test(value));
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const trimmed = (value: unknown, minimum: number, maximum: number): value is string =>
  text(value, maximum) &&
  [...value].length >= minimum &&
  value.trim() === value &&
  !/[\u0000-\u001f\u007f]/.test(value);
const email = (value: unknown): value is string =>
  text(value, 254) &&
  value.length > 0 &&
  value === value.toLowerCase() &&
  value.split('@').length === 2 &&
  value.split('@')[0]!.length > 0 &&
  value.split('@')[1]!.includes('.') &&
  !/[\u0000-\u001f\u007f\s]/.test(value);
const statuses = new Set(['pending', 'approved', 'rejected', 'revoked']);
const orderStates = new Set([
  'draft',
  'submitted',
  'confirmed',
  'partially_fulfilled',
  'fulfilled',
  'converted',
  'cancelled',
]);

const row = (value: unknown): value is NativeCustomerAccessRow =>
  exact(value, [
    'id',
    'company_id',
    'order_id',
    'customer_id',
    'email',
    'expires_at',
    'status',
    'reason',
    'created_by',
    'approved_by',
    'reviewed_at',
    'version',
    'created_at',
    'updated_at',
    'number',
    'order_state',
    'branch_id',
    'department_id',
    'reviewers',
    'canReview',
    'canReject',
    'canRevoke',
    'canInvite',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  identifier(value.order_id) &&
  identifier(value.customer_id) &&
  email(value.email) &&
  integer(value.expires_at, 1) &&
  typeof value.status === 'string' &&
  statuses.has(value.status) &&
  trimmed(value.reason, 3, 2_000) &&
  identifier(value.created_by) &&
  optionalIdentifier(value.approved_by) &&
  (value.reviewed_at === null || integer(value.reviewed_at, 1)) &&
  integer(value.version, 1) &&
  integer(value.created_at) &&
  integer(value.updated_at) &&
  trimmed(value.number, 1, 200) &&
  typeof value.order_state === 'string' &&
  orderStates.has(value.order_state) &&
  optionalIdentifier(value.branch_id) &&
  optionalIdentifier(value.department_id) &&
  integer(value.reviewers) &&
  boolean(value.canReview) &&
  boolean(value.canReject) &&
  boolean(value.canRevoke) &&
  boolean(value.canInvite);

const source = (value: unknown): value is NativeCustomerAccessSource =>
  exact(value, ['id', 'number', 'name', 'version']) &&
  identifier(value.id) &&
  trimmed(value.number, 1, 200) &&
  trimmed(value.name, 1, 300) &&
  integer(value.version, 1);

const event = (value: unknown): value is NativeCustomerAccessEvent =>
  exact(value, ['version', 'action', 'actor_id', 'reason', 'created_at']) &&
  integer(value.version, 1) &&
  value.version <= 1_000_000 &&
  trimmed(value.action, 1, 200) &&
  identifier(value.actor_id) &&
  trimmed(value.reason, 1, 2_000) &&
  integer(value.created_at);

const read = (value: unknown): value is NativeCustomerAccessRead =>
  exact(value, ['binding_version', 'user_id', 'read_at', 'projection_version']) &&
  integer(value.binding_version, 1) &&
  identifier(value.user_id) &&
  integer(value.read_at) &&
  trimmed(value.projection_version, 1, 200);

function workspace(value: unknown, actor: NativeCustomerAccessPageContext): NativeCustomerAccessWorkspace {
  if (!exact(value, ['rows', 'sources'])) return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  if (
    !Array.isArray(value.rows) ||
    value.rows.length > 200 ||
    !value.rows.every(row) ||
    !Array.isArray(value.sources) ||
    value.sources.length > 250 ||
    !value.sources.every(source)
  )
    return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  const ids = new Set<string>();
  for (const item of value.rows) {
    if (item.company_id !== actor.companyId || ids.has(item.id))
      return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
    if (actor.branchId !== null && item.branch_id !== actor.branchId)
      return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
    if (actor.departmentId !== null && item.department_id !== actor.departmentId)
      return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
    ids.add(item.id);
  }
  const sourceIds = new Set<string>();
  for (const item of value.sources) {
    if (!sourceIds.add(item.id)) return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  }
  return value as unknown as NativeCustomerAccessWorkspace;
}

function detail(
  value: unknown,
  actor: NativeCustomerAccessPageContext,
  id: string,
): NativeCustomerAccessDetail {
  if (!exact(value, ['binding', 'events', 'reads'])) return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  if (
    !row(value.binding) ||
    value.binding.id !== id ||
    value.binding.company_id !== actor.companyId ||
    (actor.branchId !== null && value.binding.branch_id !== actor.branchId) ||
    (actor.departmentId !== null && value.binding.department_id !== actor.departmentId) ||
    !Array.isArray(value.events) ||
    value.events.length > 500 ||
    !value.events.every(event) ||
    !Array.isArray(value.reads) ||
    value.reads.length > 100 ||
    !value.reads.every(read)
  )
    return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  let previous = 0;
  for (const item of value.events) {
    if (item.version <= previous) return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
    previous = item.version;
  }
  return value as unknown as NativeCustomerAccessDetail;
}

function receipt(value: unknown): NativeCustomerAccessReceipt {
  if (
    !exact(value, ['id', 'status', 'version']) ||
    !identifier(value.id) ||
    typeof value.status !== 'string' ||
    !statuses.has(value.status) ||
    !integer(value.version, 1)
  )
    return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  return value as unknown as NativeCustomerAccessReceipt;
}

const invitationStates = new Set([
  'queued',
  'running',
  'sent',
  'accepted',
  'revoked',
  'superseded',
  'expired',
  'failed',
]);

function invitationReceipt(value: unknown): NativePortalInvitationReceipt {
  if (
    !exact(value, ['id', 'bindingId', 'invitationId', 'generation', 'version', 'state', 'expiresAt']) ||
    !identifier(value.id) ||
    !identifier(value.bindingId) ||
    value.bindingId !== value.id ||
    !identifier(value.invitationId) ||
    !integer(value.generation, 1) ||
    !integer(value.version, 1) ||
    value.state !== 'queued' ||
    !integer(value.expiresAt, 1)
  )
    return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  return value as unknown as NativePortalInvitationReceipt;
}

const invitationRow = (value: unknown, id: string): value is NativePortalInvitationRow =>
  exact(value, [
    'id',
    'binding_id',
    'generation',
    'holding_slug',
    'email',
    'state',
    'token_expires_at',
    'attempts',
    'next_attempt_at',
    'last_error',
    'created_by',
    'created_at',
    'sent_at',
    'accepted_at',
    'accepted_user_id',
  ]) &&
  identifier(value.id) &&
  value.binding_id === id &&
  integer(value.generation, 1) &&
  trimmed(value.holding_slug, 1, 100) &&
  email(value.email) &&
  typeof value.state === 'string' &&
  invitationStates.has(value.state) &&
  integer(value.token_expires_at, 1) &&
  integer(value.attempts) &&
  integer(value.next_attempt_at) &&
  (value.last_error === null || trimmed(value.last_error, 1, 200)) &&
  identifier(value.created_by) &&
  integer(value.created_at) &&
  (value.sent_at === null || integer(value.sent_at)) &&
  (value.accepted_at === null || integer(value.accepted_at)) &&
  optionalIdentifier(value.accepted_user_id);

const invitationEvent = (value: unknown, id: string): value is NativePortalInvitationEvent =>
  exact(value, ['id', 'invitation_id', 'action', 'actor_id', 'reason', 'created_at']) &&
  identifier(value.id) &&
  identifier(value.invitation_id) &&
  trimmed(value.action, 1, 200) &&
  identifier(value.actor_id) &&
  text(value.reason, 2_000) &&
  integer(value.created_at);

function invitationHistory(
  value: unknown,
  id: string,
): NativePortalInvitationHistory {
  if (!exact(value, ['invitations', 'invitationEvents'])) return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  if (
    !Array.isArray(value.invitations) ||
    value.invitations.length > 200 ||
    !value.invitations.every((item) => invitationRow(item, id)) ||
    !Array.isArray(value.invitationEvents) ||
    value.invitationEvents.length > 500 ||
    !value.invitationEvents.every((item) => invitationEvent(item, id))
  )
    return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  const ids = new Set<string>();
  for (const item of value.invitations) if (!ids.add(item.id)) return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  let previous = Number.MAX_SAFE_INTEGER;
  for (const item of value.invitations) {
    if (item.generation >= previous) return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
    previous = item.generation;
  }
  return value as unknown as NativePortalInvitationHistory;
}

function context(
  value: unknown,
  actor: NativeCustomerAccessPageContext,
  input: NativeCustomerAccessReadInput,
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
      'assignmentScopeKind',
      'modules',
      'permissions',
      'expiresAt',
    ])
  )
    return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  const parsed = value as Context;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== (input.branchSlug ?? null) ||
    parsed.departmentSlug !== (input.departmentSlug ?? null) ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority?.assignmentVersion ||
    parsed.revision !== actor.authority?.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    !Array.isArray(parsed.modules) ||
    parsed.modules.length > 128 ||
    !parsed.modules.every((item) => identifier(item)) ||
    parsed.modules.length !== actor.modules.length ||
    !parsed.modules.every((item) => actor.modules.includes(item)) ||
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length > 512 ||
    !parsed.permissions.every((item) => identifier(item)) ||
    parsed.permissions.length !== (actor.iam?.permissions.length ?? 0) ||
    !parsed.permissions.every((item) => (actor.iam?.permissions ?? []).includes(item))
  )
    return failure('NATIVE_CUSTOMER_ACCESS_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_CUSTOMER_ACCESS_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeCustomerAccessConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_CUSTOMER_ACCESS_CONFIGURATION_INVALID');
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
    raw.timeoutMs > 30000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_CUSTOMER_ACCESS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_CUSTOMER_ACCESS_CONFIGURATION_INVALID');
    }
    const originPort = origin.port;
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      (originPort !== '' &&
        (!/^[1-9]\d{0,4}$/.test(originPort) ||
          Number(originPort) < 1 ||
          Number(originPort) > 65535)) ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_CUSTOMER_ACCESS_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeCustomerAccessTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeCustomerAccessPageContext,
  input: NativeCustomerAccessReadInput,
  method: 'GET' | 'POST',
  path: string,
  body: string | undefined,
  key: string | undefined,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const forwarded = incoming
    .get('x-forwarded-host')
    ?.split(',')[0]
    ?.trim();
  const host = forwarded && forwarded.length > 0 ? forwarded : incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (
    !holding ||
    holding.holdingSlug !== input.holdingSlug ||
    holding.holdingId !== actor.holdingId
  )
    return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [header, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID'));
      headers.set(header, slug);
    }
  }
  if (input.expected) {
    if (
      !identifier(input.expected.companyId) ||
      !identifier(input.expected.assignmentId) ||
      !integer(input.expected.revision) ||
      !integer(input.expected.assignmentVersion, 1)
    )
      return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  if (method === 'POST') {
    if (!key || !text(key, 180) || key.length < 8 || !/^[A-Za-z0-9:_.-]+$/.test(key))
      return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID'));
    if (body === undefined)
      return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', key);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/customer-access${path}`,
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
        return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_LIMIT');
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
    return failure('NATIVE_CUSTOMER_ACCESS_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_CUSTOMER_ACCESS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
  }
}

export function nativeCustomerAccessAvailable(): boolean {
  return process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG !== undefined;
}

export function nativeCustomerAccessConfiguration(raw: string): NativeCustomerAccessConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_CUSTOMER_ACCESS_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_CUSTOMER_ACCESS_CONFIGURATION_INVALID');
  return value as unknown as NativeCustomerAccessConfiguration;
}

export function createNativeCustomerAccessClient(
  raw: NativeCustomerAccessConfiguration,
  transport: NativeCustomerAccessTransport = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeCustomerAccessPageContext,
    input: NativeCustomerAccessReadInput,
    method: 'GET' | 'POST',
    path: string,
    body: string | undefined,
    key: string | undefined,
    signal: AbortSignal | undefined,
    validate: (value: unknown) => unknown,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_CUSTOMER_ACCESS_CONTEXT_CHANGED');
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
      if (error instanceof NativeCustomerAccessError) throw error;
      return failure('NATIVE_CUSTOMER_ACCESS_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID');
    context(value.context, actor, input);
    return validate(value.data);
  };
  return {
    workspace: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeCustomerAccessPageContext,
      input: NativeCustomerAccessReadInput,
      signal?: AbortSignal,
    ) =>
      run(incoming, actor, input, 'GET', '', undefined, undefined, signal, (value) =>
        workspace(value, actor),
      ) as Promise<NativeCustomerAccessWorkspace>,
    detail: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeCustomerAccessPageContext,
      id: string,
      input: NativeCustomerAccessReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'GET',
        `/${encodeURIComponent(id)}`,
        undefined,
        undefined,
        signal,
        (value) => detail(value, actor, id),
      ) as Promise<NativeCustomerAccessDetail>;
    },
    request: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeCustomerAccessPageContext,
      body: string,
      key: string,
      input: NativeCustomerAccessReadInput,
      signal?: AbortSignal,
    ) =>
      run(incoming, actor, input, 'POST', '', body, key, signal, receipt) as Promise<NativeCustomerAccessReceipt>,
    change: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeCustomerAccessPageContext,
      id: string,
      body: string,
      key: string,
      input: NativeCustomerAccessReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'POST',
        `/${encodeURIComponent(id)}`,
        body,
        key,
        signal,
        receipt,
      ) as Promise<NativeCustomerAccessReceipt>;
    },
    invitation: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeCustomerAccessPageContext,
      id: string,
      body: string,
      key: string,
      input: NativeCustomerAccessReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'POST',
        `/${encodeURIComponent(id)}/invitations`,
        body,
        key,
        signal,
        invitationReceipt,
      ) as Promise<NativePortalInvitationReceipt>;
    },
    invitationHistory: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeCustomerAccessPageContext,
      id: string,
      input: NativeCustomerAccessReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'GET',
        `/${encodeURIComponent(id)}/invitations`,
        undefined,
        undefined,
        signal,
        (value) => invitationHistory(value, id),
      ) as Promise<NativePortalInvitationHistory>;
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
