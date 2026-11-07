import 'server-only';
import { Agent, request } from 'node:http';
import { allowedPrivateLoopbackPort } from '@erpx/native-sdk/loopback';

/** Rendering input from the retained route authorization, never a grant. */
export type NativeLegalPageContext = {
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

export type NativeLegalConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  production?: boolean;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

type Row = Record<string, unknown>;
export type NativeLegalKind =
  | 'request'
  | 'matter'
  | 'contract'
  | 'obligation'
  | 'compliance'
  | 'license'
  | 'corporate'
  | 'ip'
  | 'privacy'
  | 'counsel'
  | 'template'
  | 'rule';
export type NativeLegalClassification = 'internal' | 'confidential' | 'restricted';
export type NativeLegalRecord = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  kind: NativeLegalKind;
  title: string;
  description: string;
  classification: NativeLegalClassification;
  country: string;
  jurisdiction: string;
  language: string;
  currency: string;
  timezone: string;
  priority: string;
  due_date: string | null;
  owner_id: string;
  created_by: string;
  state: string;
  version: number;
  created_at: number;
  updated_at: number;
  data: Record<string, string | number>;
};
export type NativeLegalDocument = {
  id: string;
  item_id: string;
  version: number;
  name: string;
  media_type: string;
  checksum: string;
  bytes: number;
  body: string;
  created_by: string;
  created_at: number;
  locked: boolean;
  object_key: string | null;
};
export type NativeLegalEvent = {
  id: string;
  item_id: string;
  action: string;
  actor_id: string;
  note: string;
  version: number;
  created_at: number;
};
export type NativeLegalMember = { user_id: string; access: string };
export type NativeLegalHold = {
  id: string;
  reason: string;
  state: string;
  direct: boolean;
  acknowledged_at: number | null;
};
export type NativeLegalLink = { target_id: string; relation: string; title: string };
export type NativeLegalComment = {
  id: string;
  body: string;
  visibility: string;
  created_by: string;
  created_at: number;
};
export type NativeLegalSigner = {
  user_id: string;
  ordinal: number;
  state: string;
  signed_at: number | null;
};
export type NativeLegalEnvelope = {
  id: string;
  state: string;
  document_id: string;
  expires_at: number;
  consent: string;
  signers: NativeLegalSigner[];
};
export type NativeLegalShare = {
  id: string;
  email: string;
  document_id: string;
  user_id: string | null;
  expires_at: number;
  revoked_at: number | null;
};
export type NativeLegalSubmission = {
  id: string;
  name: string;
  checksum: string;
  created_by: string;
  created_at: number;
};
export type NativeLegalAccess = {
  id: string;
  document_id: string | null;
  actor_id: string;
  action: string;
  created_at: number;
};
export type NativeLegalRegister = {
  records: NativeLegalRecord[];
  total: number;
  page: number;
  counts: { state: string; count: number }[];
  enabled: boolean;
  can_edit: boolean;
  can_hold: boolean;
  can_release_hold: boolean;
  can_share: boolean;
};
export type NativeLegalDetail = {
  item: NativeLegalRecord;
  documents: NativeLegalDocument[];
  events: NativeLegalEvent[];
  members: NativeLegalMember[];
  holds: NativeLegalHold[];
  links: NativeLegalLink[];
  comments: NativeLegalComment[];
  envelopes: NativeLegalEnvelope[];
  shares: NativeLegalShare[];
  submissions: NativeLegalSubmission[];
  access: NativeLegalAccess[];
  can_edit: boolean;
  can_hold: boolean;
  can_release_hold: boolean;
  can_share: boolean;
};
export type NativeLegalCommandResult = {
  id: string;
  version: number;
  state: string;
  documentId?: string;
  holdId?: string;
  executedCopyId?: string;
  executedCopyState?: string;
  jobId?: string;
};

export class NativeLegalError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Legal request could not be completed.');
    this.name = 'NativeLegalError';
    this.code = code;
    this.status = status;
  }
}

type Transport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

const fail = (code: string, status?: number, message?: string): never => {
  throw new NativeLegalError(code, status, message);
};
const object = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  object(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const optionalText = (value: unknown, max = 4096): value is string | null =>
  value === null || text(value, max);
const nullableIdentifier = (value: unknown): value is string | null =>
  value === null || identifier(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const kind = (value: unknown): value is NativeLegalKind =>
  typeof value === 'string' &&
  [
    'request',
    'matter',
    'contract',
    'obligation',
    'compliance',
    'license',
    'corporate',
    'ip',
    'privacy',
    'counsel',
    'template',
    'rule',
  ].includes(value);
const state = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-z][a-z0-9_-]{0,39}$/.test(value);
const classification = (value: unknown): value is NativeLegalClassification =>
  ['internal', 'confidential', 'restricted'].includes(String(value));

function fields(value: unknown, keys: readonly string[]): Row {
  if (!exact(value, keys)) return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return value;
}

function record(value: unknown): NativeLegalRecord {
  const row = fields(value, [
    'id',
    'company_id',
    'branch_id',
    'department_id',
    'kind',
    'title',
    'description',
    'classification',
    'country',
    'jurisdiction',
    'language',
    'currency',
    'timezone',
    'priority',
    'due_date',
    'owner_id',
    'created_by',
    'state',
    'version',
    'created_at',
    'updated_at',
    'data',
  ]);
  if (
    !identifier(row.id) ||
    !identifier(row.company_id) ||
    !nullableIdentifier(row.branch_id) ||
    !nullableIdentifier(row.department_id) ||
    !kind(row.kind) ||
    !text(row.title, 240) ||
    !text(row.description, 20000) ||
    !classification(row.classification) ||
    !text(row.country, 2) ||
    !text(row.jurisdiction, 200) ||
    !text(row.language, 40) ||
    !text(row.currency, 3) ||
    !text(row.timezone, 80) ||
    !text(row.priority, 20) ||
    !optionalText(row.due_date, 40) ||
    !identifier(row.owner_id) ||
    !identifier(row.created_by) ||
    !state(row.state) ||
    !integer(row.version, 1) ||
    !integer(row.created_at) ||
    !integer(row.updated_at) ||
    !legalData(row.data)
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalRecord;
}

function legalData(value: unknown): value is Row {
  if (!object(value) || Object.keys(value).length > 100) return false;
  return Object.entries(value).every(([key, item]) => {
    if (!/^[A-Za-z][A-Za-z0-9_]{0,79}$/.test(key)) return false;
    if (typeof item === 'string') return new TextEncoder().encode(item).length <= 50000;
    return integer(item);
  });
}

function document(value: unknown): NativeLegalDocument {
  const row = fields(value, [
    'id',
    'item_id',
    'version',
    'name',
    'media_type',
    'checksum',
    'bytes',
    'body',
    'created_by',
    'created_at',
    'locked',
    'object_key',
  ]);
  if (
    !identifier(row.id) ||
    !identifier(row.item_id) ||
    !integer(row.version, 1) ||
    !text(row.name, 200) ||
    !text(row.media_type, 200) ||
    !text(row.checksum, 64) ||
    !/^[a-f0-9]{64}$/.test(row.checksum) ||
    !integer(row.bytes) ||
    !text(row.body, 200000) ||
    !identifier(row.created_by) ||
    !integer(row.created_at) ||
    !boolean(row.locked) ||
    !optionalText(row.object_key, 500)
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalDocument;
}

function event(value: unknown): NativeLegalEvent {
  const row = fields(value, [
    'id',
    'item_id',
    'action',
    'actor_id',
    'note',
    'version',
    'created_at',
  ]);
  if (
    !identifier(row.id) ||
    !identifier(row.item_id) ||
    !text(row.action, 200) ||
    !identifier(row.actor_id) ||
    !text(row.note, 4000) ||
    !integer(row.version, 1) ||
    !integer(row.created_at)
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalEvent;
}

function member(value: unknown): NativeLegalMember {
  const row = fields(value, ['user_id', 'access']);
  if (!identifier(row.user_id) || !['read', 'edit'].includes(String(row.access)))
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalMember;
}

function hold(value: unknown): NativeLegalHold {
  const row = fields(value, ['id', 'reason', 'state', 'direct', 'acknowledged_at']);
  if (
    !identifier(row.id) ||
    !text(row.reason, 4000) ||
    !['active', 'release_requested', 'released'].includes(String(row.state)) ||
    !boolean(row.direct) ||
    (row.acknowledged_at !== null && !integer(row.acknowledged_at))
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalHold;
}

function link(value: unknown): NativeLegalLink {
  const row = fields(value, ['target_id', 'relation', 'title']);
  if (
    !identifier(row.target_id) ||
    !['related', 'evidence', 'amends', 'obligation', 'derived'].includes(String(row.relation)) ||
    !text(row.title, 240)
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalLink;
}

function comment(value: unknown): NativeLegalComment {
  const row = fields(value, ['id', 'body', 'visibility', 'created_by', 'created_at']);
  if (
    !identifier(row.id) ||
    !text(row.body, 20000) ||
    !['internal', 'external'].includes(String(row.visibility)) ||
    !identifier(row.created_by) ||
    !integer(row.created_at)
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalComment;
}

function signer(value: unknown): NativeLegalSigner {
  const row = fields(value, ['user_id', 'ordinal', 'state', 'signed_at']);
  if (
    !identifier(row.user_id) ||
    !integer(row.ordinal) ||
    !['pending', 'signed', 'declined'].includes(String(row.state)) ||
    (row.signed_at !== null && !integer(row.signed_at))
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalSigner;
}

function envelope(value: unknown): NativeLegalEnvelope {
  const row = fields(value, ['id', 'state', 'document_id', 'expires_at', 'consent', 'signers']);
  if (
    !identifier(row.id) ||
    !['sent', 'completed', 'declined', 'cancelled', 'expired'].includes(String(row.state)) ||
    !identifier(row.document_id) ||
    !integer(row.expires_at) ||
    !text(row.consent, 20000) ||
    !Array.isArray(row.signers) ||
    row.signers.length > 20
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return { ...(row as NativeLegalEnvelope), signers: row.signers.map(signer) };
}

function share(value: unknown): NativeLegalShare {
  const row = fields(value, [
    'id',
    'email',
    'document_id',
    'user_id',
    'expires_at',
    'revoked_at',
  ]);
  if (
    !identifier(row.id) ||
    !text(row.email, 320) ||
    !identifier(row.document_id) ||
    !nullableIdentifier(row.user_id) ||
    !integer(row.expires_at) ||
    (row.revoked_at !== null && !integer(row.revoked_at))
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalShare;
}

function submission(value: unknown): NativeLegalSubmission {
  const row = fields(value, ['id', 'name', 'checksum', 'created_by', 'created_at']);
  if (
    !identifier(row.id) ||
    !text(row.name, 200) ||
    !text(row.checksum, 64) ||
    !/^[a-f0-9]{64}$/.test(row.checksum) ||
    !identifier(row.created_by) ||
    !integer(row.created_at)
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalSubmission;
}

function access(value: unknown): NativeLegalAccess {
  const row = fields(value, ['id', 'document_id', 'actor_id', 'action', 'created_at']);
  if (
    !identifier(row.id) ||
    !nullableIdentifier(row.document_id) ||
    !identifier(row.actor_id) ||
    !text(row.action, 200) ||
    !integer(row.created_at)
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return row as NativeLegalAccess;
}

function contextMatches(value: unknown, actor: NativeLegalPageContext): boolean {
  if (!object(value)) return false;
  return (
    value.userId === actor.userId &&
    value.holdingId === actor.holdingId &&
    value.companyId === actor.companyId &&
    value.branchId === actor.branchId &&
    value.departmentId === actor.departmentId &&
    value.assignmentId === actor.assignmentId &&
    value.revision === actor.authority?.revision &&
    value.assignmentVersion === actor.authority?.assignmentVersion &&
    integer(value.expiresAt, 1)
  );
}

function register(value: unknown, actor: NativeLegalPageContext): NativeLegalRegister {
  if (!exact(value, ['data', 'context']) || !contextMatches(value.context, actor)) {
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  }
  const data = fields(value.data, [
    'records',
    'total',
    'page',
    'counts',
    'enabled',
    'can_edit',
    'can_hold',
    'can_release_hold',
    'can_share',
  ]);
  if (
    !Array.isArray(data.records) ||
    data.records.length > 30 ||
    !integer(data.total) ||
    !integer(data.page, 1) ||
    data.page > 100000 ||
    !Array.isArray(data.counts) ||
    data.counts.length > 100 ||
    !boolean(data.enabled) ||
    !boolean(data.can_edit) ||
    !boolean(data.can_hold) ||
    !boolean(data.can_release_hold) ||
    !boolean(data.can_share)
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return {
    records: data.records.map(record),
    total: data.total,
    page: data.page,
    counts: data.counts.map((value) => {
      const count = fields(value, ['state', 'count']);
      if (!state(count.state) || !integer(count.count))
        return fail('NATIVE_LEGAL_RESPONSE_INVALID');
      return count as { state: string; count: number };
    }),
    enabled: data.enabled,
    can_edit: data.can_edit,
    can_hold: data.can_hold,
    can_release_hold: data.can_release_hold,
    can_share: data.can_share,
  };
}

function rows(value: unknown, limit: number): unknown[] {
  if (!Array.isArray(value) || value.length > limit)
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return value;
}

function detail(value: unknown, actor: NativeLegalPageContext): NativeLegalDetail {
  if (!exact(value, ['data', 'context']) || !contextMatches(value.context, actor)) {
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  }
  const data = fields(value.data, [
    'item',
    'documents',
    'events',
    'members',
    'holds',
    'links',
    'comments',
    'envelopes',
    'shares',
    'submissions',
    'access',
    'can_edit',
    'can_hold',
    'can_release_hold',
    'can_share',
  ]);
  if (
    !boolean(data.can_edit) ||
    !boolean(data.can_hold) ||
    !boolean(data.can_release_hold) ||
    !boolean(data.can_share)
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return {
    item: record(data.item),
    documents: rows(data.documents, 100).map(document),
    events: rows(data.events, 100).map(event),
    members: rows(data.members, 100).map(member),
    holds: rows(data.holds, 100).map(hold),
    links: rows(data.links, 100).map(link),
    comments: rows(data.comments, 100).map(comment),
    envelopes: rows(data.envelopes, 20).map(envelope),
    shares: rows(data.shares, 100).map(share),
    submissions: rows(data.submissions, 100).map(submission),
    access: rows(data.access, 100).map(access),
    can_edit: data.can_edit,
    can_hold: data.can_hold,
    can_release_hold: data.can_release_hold,
    can_share: data.can_share,
  };
}

function commandResult(value: unknown, actor: NativeLegalPageContext): NativeLegalCommandResult {
  if (!exact(value, ['data', 'context']) || !contextMatches(value.context, actor)) {
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  }
  const data = value.data;
  if (!object(data)) return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  const allowed = ['id', 'version', 'state', 'documentId', 'holdId', 'executedCopyId', 'executedCopyState', 'jobId'];
  if (Object.keys(data).some((key) => !allowed.includes(key)))
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  if (
    !identifier(data.id) ||
    !integer(data.version, 1) ||
    !state(data.state) ||
    (data.documentId !== undefined && !identifier(data.documentId)) ||
    (data.holdId !== undefined && !identifier(data.holdId)) ||
    (data.executedCopyId !== undefined && !identifier(data.executedCopyId)) ||
    (data.executedCopyState !== undefined && !state(data.executedCopyState)) ||
    (data.jobId !== undefined && !identifier(data.jobId))
  )
    return fail('NATIVE_LEGAL_RESPONSE_INVALID');
  return data as NativeLegalCommandResult;
}

function configuration(raw: NativeLegalConfiguration): NativeLegalConfiguration {
  if (
    typeof raw?.privateBaseUrl !== 'string' ||
    (raw.production !== undefined && typeof raw.production !== 'boolean') ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    throw new NativeLegalError('NATIVE_LEGAL_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    throw new NativeLegalError('NATIVE_LEGAL_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    !allowedPrivateLoopbackPort(port, raw.production === true) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin
  )
    throw new NativeLegalError('NATIVE_LEGAL_CONFIGURATION_INVALID');
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  const ids = new Set<string>();
  const holdings = raw.holdings.map((holding) => {
    if (
      !object(holding) ||
      typeof holding.origin !== 'string' ||
      !identifier(holding.holdingSlug) ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug) ||
      !identifier(holding.holdingId)
    )
      throw new NativeLegalError('NATIVE_LEGAL_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      throw new NativeLegalError('NATIVE_LEGAL_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== holding.origin ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug) ||
      ids.has(holding.holdingId)
    )
      throw new NativeLegalError('NATIVE_LEGAL_CONFIGURATION_INVALID');
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
    ids.add(holding.holdingId);
    return { origin: holding.origin, holdingSlug: holding.holdingSlug, holdingId: holding.holdingId };
  });
  return {
    privateBaseUrl: raw.privateBaseUrl,
    timeoutMs: raw.timeoutMs,
    production: raw.production === true,
    holdings,
  };
}

function directHttp(
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
      { method, agent, headers: Object.fromEntries(headers), signal },
      (response) => {
        const chunks: Buffer[] = [];
        response.on('data', (chunk) => chunks.push(Buffer.from(chunk)));
        response.on('end', () => {
          const reply = new Response(Buffer.concat(chunks), {
            status: response.statusCode ?? 500,
            headers: response.headers as HeadersInit,
          });
          agent.destroy();
          resolve(reply);
        });
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    if (body !== undefined) outgoing.write(body);
    outgoing.end();
  });
}

async function boundedJson(response: Response): Promise<unknown> {
  if (!response.body) throw new NativeLegalError('NATIVE_LEGAL_RESPONSE_INVALID');
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > 24 * 1024 * 1024)
        throw new NativeLegalError('NATIVE_LEGAL_RESPONSE_LIMIT');
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const text = new TextDecoder().decode(Buffer.concat(chunks.map((chunk) => Buffer.from(chunk))));
  try {
    return JSON.parse(text);
  } catch {
    throw new NativeLegalError('NATIVE_LEGAL_RESPONSE_INVALID');
  }
}

function responseHeaders(
  incoming: Pick<Headers, 'get'>,
  actor: NativeLegalPageContext,
  body: boolean,
): Headers {
  const forwarded = incoming
    .get('x-forwarded-host')
    ?.split(',')[0]
    ?.trim();
  const host = forwarded && forwarded.length > 0 ? forwarded : incoming.get('host');
  if (!host) throw new NativeLegalError('NATIVE_LEGAL_AUTHORITY_INVALID');
  const headers = new Headers({
    host,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      throw new NativeLegalError('NATIVE_LEGAL_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  const origin = incoming.get('origin');
  if (origin) headers.set('origin', origin);
  if (!actor.authority || !actor.iam) {
    throw new NativeLegalError('NATIVE_LEGAL_CONTEXT_CHANGED');
  }
  headers.set('x-erpx-context-company', actor.companyId);
  headers.set('x-erpx-context-assignment', actor.assignmentId);
  headers.set('x-erpx-context-revision', String(actor.authority.revision));
  headers.set(
    'x-erpx-context-assignment-version',
    String(actor.authority.assignmentVersion),
  );
  if ('branchSlug' in actor.scope && actor.scope.branchSlug)
    headers.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department')
    headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  if (body) headers.set('content-type', 'application/json');
  return headers;
}

async function call(
  transport: Transport,
  config: NativeLegalConfiguration,
  incoming: Pick<Headers, 'get'>,
  actor: NativeLegalPageContext,
  path: string,
  body: string | undefined,
  idempotencyKey?: string,
  origin?: string,
): Promise<unknown> {
  const holding = config.holdings.find(
    (candidate) =>
      candidate.holdingSlug === actor.holdingSlug && candidate.holdingId === actor.holdingId,
  );
  if (!holding) throw new NativeLegalError('NATIVE_LEGAL_CONTEXT_CHANGED');
  const headers = responseHeaders(incoming, actor, body !== undefined);
  if (body !== undefined) {
    if (new TextEncoder().encode(body).length > 512 * 1024)
      throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
    if (!idempotencyKey || !/^[A-Za-z0-9:_.-]{8,180}$/.test(idempotencyKey))
      throw new NativeLegalError('NATIVE_LEGAL_COMMAND_KEY_INVALID');
    headers.set('idempotency-key', idempotencyKey);
    if (!origin) throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
    let originUrl: URL;
    try {
      originUrl = new URL(origin);
    } catch {
      throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
    }
    if (origin !== originUrl.origin || origin !== holding.origin)
      throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
    headers.set('origin', origin);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/legal${path}`,
    config.privateBaseUrl,
  );
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await transport(
      body === undefined ? 'GET' : 'POST',
      url,
      headers,
      body,
      controller.signal,
    );
    if (!response.ok) {
      let code = 'NATIVE_LEGAL_UNAVAILABLE';
      let message: string | undefined;
      try {
        const error = await boundedJson(response);
        if (object(error) && object(error.error)) {
          if (typeof error.error.code === 'string' && /^[A-Z][A-Z0-9_]{2,80}$/.test(error.error.code))
            code = error.error.code;
          if (typeof error.error.message === 'string' && error.error.message.length <= 400)
            message = error.error.message;
        }
      } catch {
        /* Retain the bounded status failure. */
      }
      throw new NativeLegalError(code, response.status, message);
    }
    return await boundedJson(response);
  } catch (error) {
    if (error instanceof NativeLegalError) throw error;
    throw new NativeLegalError('NATIVE_LEGAL_UNAVAILABLE');
  } finally {
    clearTimeout(timer);
  }
}

export function nativeLegalConfiguration(raw: string): NativeLegalConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeLegalError('NATIVE_LEGAL_CONFIGURATION_INVALID');
  }
  if (!object(value)) throw new NativeLegalError('NATIVE_LEGAL_CONFIGURATION_INVALID');
  return configuration(value as unknown as NativeLegalConfiguration);
}

export function createNativeLegalClient(
  raw: NativeLegalConfiguration,
  transport: Transport = directHttp,
) {
  const config = configuration(raw);
  return {
    async register(
      incoming: Pick<Headers, 'get'>,
      actor: NativeLegalPageContext,
      query: URLSearchParams,
      signal?: AbortSignal,
    ): Promise<NativeLegalRegister> {
      void signal;
      return register(
        await call(
          transport,
          config,
          incoming,
          actor,
          `/records?${query.toString()}`,
          undefined,
        ),
        actor,
      );
    },
    async detail(
      incoming: Pick<Headers, 'get'>,
      actor: NativeLegalPageContext,
      id: string,
      signal?: AbortSignal,
    ): Promise<NativeLegalDetail> {
      void signal;
      if (!identifier(id)) throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
      return detail(
        await call(transport, config, incoming, actor, `/records/${encodeURIComponent(id)}`, undefined),
        actor,
      );
    },
    async command(
      incoming: Pick<Headers, 'get'>,
      actor: NativeLegalPageContext,
      body: unknown,
      idempotencyKey: string,
    ): Promise<NativeLegalCommandResult> {
      if (!object(body)) throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
      return commandResult(
        await call(
          transport,
          config,
          incoming,
          actor,
          '/commands',
          JSON.stringify(body),
          idempotencyKey,
          incoming.get('origin') ?? undefined,
        ),
        actor,
      );
    },
  };
}

export function nativeLegalFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeLegalError)) return undefined;
  if (error.code === 'NATIVE_LEGAL_RESPONSE_INVALID' || error.code === 'NATIVE_LEGAL_RESPONSE_LIMIT')
    return {
      status: 502,
      code: 'NATIVE_LEGAL_RESPONSE_INVALID',
      message: 'The Legal reply was invalid.',
    };
  if (error.code === 'NATIVE_LEGAL_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before submitting.',
    };
  if (
    error.code === 'NATIVE_LEGAL_REQUEST_INVALID' ||
    error.code === 'NATIVE_LEGAL_COMMAND_KEY_INVALID' ||
    error.code === 'NATIVE_LEGAL_COOKIE_LIMIT' ||
    error.code === 'NATIVE_LEGAL_AUTHORITY_INVALID'
  )
    return {
      status: error.code === 'NATIVE_LEGAL_AUTHORITY_INVALID' ? 403 : 422,
      code: error.code === 'NATIVE_LEGAL_AUTHORITY_INVALID' ? 'FORBIDDEN' : 'NATIVE_LEGAL_REQUEST_INVALID',
      message:
        error.code === 'NATIVE_LEGAL_AUTHORITY_INVALID'
          ? 'This action is unavailable in your current role.'
          : 'Review the Legal request and submit it again.',
    };
  if (error.code === 'NATIVE_LEGAL_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_LEGAL_UNAVAILABLE',
      message: 'The Legal service is unavailable.',
    };
  if (error.status) return { status: error.status, code: error.code, message: error.message };
  return { status: 503, code: 'NATIVE_LEGAL_UNAVAILABLE', message: 'The Legal service is unavailable.' };
}
