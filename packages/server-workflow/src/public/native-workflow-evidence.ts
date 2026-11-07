import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering and read authority from retained page authorization, never a grant. */
export type NativeWorkflowEvidenceActor = Readonly<{
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  scopeKind: string;
  authority: { revision: number; assignmentVersion: number };
  permissions: readonly string[];
}>;

export type NativeWorkflowEvidenceConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;

export type NativeWorkflowApprovalKind =
  | 'expense'
  | 'leave'
  | 'leave_cancellation'
  | 'task'
  | 'overtime';

export type NativeWorkflowApprovalQueueRow = Readonly<{
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  record_type: NativeWorkflowApprovalKind;
  record_id: string;
  state: 'pending' | 'blocked';
  blocked_reason: string | null;
  version: number;
  created_at: number;
}>;

export type NativeWorkflowApprovalQueue = Readonly<{
  evaluatedAt: number;
  approvals: NativeWorkflowApprovalQueueRow[];
}>;

export type NativeWorkflowApprovalInstance = Readonly<{
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  record_type: NativeWorkflowApprovalKind;
  record_id: string;
  phase: 'request' | 'start' | 'completion';
  cycle: number;
  requester_user_id: string;
  assignee_user_id: string | null;
  state: 'pending' | 'blocked' | 'approved' | 'rejected' | 'cancelled' | 'superseded';
  current_step: number;
  blocked_reason: string | null;
  version: number;
  created_at: number;
  updated_at: number;
}>;

export type NativeWorkflowApprovalStep = Readonly<{
  id: string;
  instance_id: string;
  ordinal: number;
  approver_user_id: string;
  authority: 'named' | 'department' | 'hr_admin' | 'holding_owner';
  state: 'pending' | 'approved' | 'rejected';
  approver_pool_json: string | null;
  required_role: string | null;
  stage: string | null;
  route_snapshot_json: string | null;
}>;

export type NativeWorkflowApprovalDecision = Readonly<{
  id: string;
  instance_id: string;
  step_id: string | null;
  action: 'approve' | 'reject' | 'cancel' | 'revise' | 'reroute' | 'initialize' | 'override';
  actor_user_id: string;
  previous_approver_user_id: string | null;
  approver_user_id: string | null;
  reason: string;
  reviewed_version: number;
  command_id: string;
  created_at: number;
}>;

export type NativeWorkflowCommandReceipt = Readonly<{
  command_id: string;
  operation: string;
  actor_id: string;
  record_id: string;
  request_hash: string;
  result_json: string | null;
  created_at: number;
}>;

export type NativeWorkflowAuditEvent = Readonly<{
  id: string;
  command_id: string;
  actor_id: string;
  assignment_id: string;
  action: string;
  record_type: string;
  record_id: string;
  record_version: number;
  purpose: string;
  created_at: number;
}>;

export type NativeWorkflowOutboxEvent = Readonly<{
  id: string;
  command_id: string;
  event_type: string;
  aggregate_type: string;
  aggregate_id: string;
  aggregate_version: number;
  created_at: number;
}>;

export type NativeWorkflowApprovalEvidence = Readonly<{
  instance: NativeWorkflowApprovalInstance;
  steps: NativeWorkflowApprovalStep[];
  history: NativeWorkflowApprovalDecision[];
  receipts: NativeWorkflowCommandReceipt[];
  audit: NativeWorkflowAuditEvent[];
  outbox: NativeWorkflowOutboxEvent[];
}>;

export type NativeWorkflowApprovalEvidenceWorkspace = Readonly<{
  evaluatedAt: number;
  approval: NativeWorkflowApprovalEvidence | null;
}>;

export type NativeWorkflowEvidenceTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeWorkflowEvidenceError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Workflow approval evidence request could not be completed.');
    this.name = 'NativeWorkflowEvidenceError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeWorkflowEvidenceError(code, status, message);
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
  text(value, 200) && value.length > 0 && !/[\u0000-\u0020\u007f]/.test(value);
const optionalIdentifier = (value: unknown): value is string | null =>
  value === null || identifier(value);
const reason = (value: unknown): value is string =>
  text(value, 2_000) && !value.includes('\u0000');
const optionalReason = (value: unknown): value is string | null =>
  value === null || reason(value);
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
const json = (value: unknown): value is string | null => {
  if (value === null) return true;
  if (!text(value, 64 * 1024)) return false;
  try {
    JSON.parse(value);
    return true;
  } catch {
    return false;
  }
};
const pool = (value: unknown): value is string | null => {
  if (value === null) return true;
  if (!text(value, 64 * 1024)) return false;
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) && parsed.every((item) => typeof item === 'string');
  } catch {
    return false;
  }
};
const hash = (value: unknown): value is string =>
  typeof value === 'string' && /^[0-9a-f]{64}$/i.test(value);
const kinds = new Set([
  'expense',
  'leave',
  'leave_cancellation',
  'task',
  'overtime',
]);
const phases = new Set(['request', 'start', 'completion']);
const instanceStates = new Set([
  'pending',
  'blocked',
  'approved',
  'rejected',
  'cancelled',
  'superseded',
]);
const stepStates = new Set(['pending', 'approved', 'rejected']);
const authorities = new Set(['named', 'department', 'hr_admin', 'holding_owner']);
const actions = new Set([
  'approve',
  'reject',
  'cancel',
  'revise',
  'reroute',
  'initialize',
  'override',
]);
const queueStates = new Set(['pending', 'blocked']);

const queueRow = (value: unknown): value is NativeWorkflowApprovalQueueRow => {
  if (
    !exact(value, [
      'id',
      'company_id',
      'branch_id',
      'department_id',
      'record_type',
      'record_id',
      'state',
      'blocked_reason',
      'version',
      'created_at',
    ])
  )
    return false;
  const row = value as Row;
  return (
    identifier(row.id) &&
    identifier(row.company_id) &&
    optionalIdentifier(row.branch_id) &&
    optionalIdentifier(row.department_id) &&
    typeof row.record_type === 'string' &&
    kinds.has(row.record_type) &&
    identifier(row.record_id) &&
    typeof row.state === 'string' &&
    queueStates.has(row.state) &&
    optionalReason(row.blocked_reason) &&
    (row.state !== 'blocked' || row.blocked_reason !== null) &&
    integer(row.version, 1) &&
    integer(row.created_at)
  );
};

const instance = (
  value: unknown,
  actor: NativeWorkflowEvidenceActor,
  kind: string,
  recordId: string,
): value is NativeWorkflowApprovalInstance => {
  if (
    !exact(value, [
      'id',
      'company_id',
      'branch_id',
      'department_id',
      'record_type',
      'record_id',
      'phase',
      'cycle',
      'requester_user_id',
      'assignee_user_id',
      'state',
      'current_step',
      'blocked_reason',
      'version',
      'created_at',
      'updated_at',
    ])
  )
    return false;
  const row = value as Row;
  return (
    identifier(row.id) &&
    row.company_id === actor.companyId &&
    optionalIdentifier(row.branch_id) &&
    optionalIdentifier(row.department_id) &&
    row.record_type === kind &&
    row.record_id === recordId &&
    typeof row.phase === 'string' &&
    phases.has(row.phase) &&
    integer(row.cycle) &&
    identifier(row.requester_user_id) &&
    optionalIdentifier(row.assignee_user_id) &&
    typeof row.state === 'string' &&
    instanceStates.has(row.state) &&
    integer(row.current_step) &&
    optionalReason(row.blocked_reason) &&
    (row.state !== 'blocked' || row.blocked_reason !== null) &&
    integer(row.version, 1) &&
    integer(row.created_at) &&
    integer(row.updated_at)
  );
};

const step = (value: unknown, instanceId: string): value is NativeWorkflowApprovalStep => {
  if (
    !exact(value, [
      'id',
      'instance_id',
      'ordinal',
      'approver_user_id',
      'authority',
      'state',
      'approver_pool_json',
      'required_role',
      'stage',
      'route_snapshot_json',
    ])
  )
    return false;
  const row = value as Row;
  return (
    identifier(row.id) &&
    row.instance_id === instanceId &&
    integer(row.ordinal) &&
    identifier(row.approver_user_id) &&
    typeof row.authority === 'string' &&
    authorities.has(row.authority) &&
    typeof row.state === 'string' &&
    stepStates.has(row.state) &&
    pool(row.approver_pool_json) &&
    (row.required_role === null || text(row.required_role, 200)) &&
    (row.stage === null || text(row.stage, 200)) &&
    json(row.route_snapshot_json)
  );
};

const decision = (
  value: unknown,
  instanceId: string,
  commandIds: Set<string>,
): value is NativeWorkflowApprovalDecision => {
  if (
    !exact(value, [
      'id',
      'instance_id',
      'step_id',
      'action',
      'actor_user_id',
      'previous_approver_user_id',
      'approver_user_id',
      'reason',
      'reviewed_version',
      'command_id',
      'created_at',
    ])
  )
    return false;
  const row = value as Row;
  if (!identifier(row.command_id)) return false;
  commandIds.add(row.command_id);
  return (
    identifier(row.id) &&
    row.instance_id === instanceId &&
    optionalIdentifier(row.step_id) &&
    typeof row.action === 'string' &&
    actions.has(row.action) &&
    identifier(row.actor_user_id) &&
    optionalIdentifier(row.previous_approver_user_id) &&
    optionalIdentifier(row.approver_user_id) &&
    reason(row.reason) &&
    integer(row.reviewed_version) &&
    integer(row.created_at)
  );
};

const receipt = (
  value: unknown,
  recordId: string,
  commandIds: Set<string>,
): value is NativeWorkflowCommandReceipt => {
  if (
    !exact(value, [
      'command_id',
      'operation',
      'actor_id',
      'record_id',
      'request_hash',
      'result_json',
      'created_at',
    ])
  )
    return false;
  const row = value as Row;
  return (
    identifier(row.command_id) &&
    commandIds.has(row.command_id) &&
    identifier(row.operation) &&
    identifier(row.actor_id) &&
    row.record_id === recordId &&
    hash(row.request_hash) &&
    json(row.result_json) &&
    integer(row.created_at)
  );
};

const auditEvent = (
  value: unknown,
  recordId: string,
  commandIds: Set<string>,
): value is NativeWorkflowAuditEvent => {
  if (
    !exact(value, [
      'id',
      'command_id',
      'actor_id',
      'assignment_id',
      'action',
      'record_type',
      'record_id',
      'record_version',
      'purpose',
      'created_at',
    ])
  )
    return false;
  const row = value as Row;
  return (
    identifier(row.id) &&
    identifier(row.command_id) &&
    commandIds.has(row.command_id) &&
    identifier(row.actor_id) &&
    identifier(row.assignment_id) &&
    identifier(row.action) &&
    identifier(row.record_type) &&
    row.record_id === recordId &&
    integer(row.record_version) &&
    identifier(row.purpose) &&
    integer(row.created_at)
  );
};

const outboxEvent = (
  value: unknown,
  recordId: string,
  commandIds: Set<string>,
): value is NativeWorkflowOutboxEvent => {
  if (
    !exact(value, [
      'id',
      'command_id',
      'event_type',
      'aggregate_type',
      'aggregate_id',
      'aggregate_version',
      'created_at',
    ])
  )
    return false;
  const row = value as Row;
  return (
    identifier(row.id) &&
    identifier(row.command_id) &&
    commandIds.has(row.command_id) &&
    identifier(row.event_type) &&
    identifier(row.aggregate_type) &&
    row.aggregate_id === recordId &&
    integer(row.aggregate_version) &&
    integer(row.created_at)
  );
};

function strictlyIncreasing(values: readonly number[]): boolean {
  for (let index = 1; index < values.length; index += 1)
    if (values[index]! < values[index - 1]!) return false;
  return true;
}
function nonIncreasing(values: readonly number[]): boolean {
  for (let index = 1; index < values.length; index += 1)
    if (values[index]! > values[index - 1]!) return false;
  return true;
}

function unique(values: readonly string[]): boolean {
  return new Set(values).size === values.length;
}

function queue(value: unknown, actor: NativeWorkflowEvidenceActor): NativeWorkflowApprovalQueue {
  if (!exact(value, ['evaluatedAt', 'approvals'])) return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const parsed = value as { evaluatedAt: unknown; approvals: unknown };
  if (
    !integer(parsed.evaluatedAt) ||
    !Array.isArray(parsed.approvals) ||
    parsed.approvals.length > 50 ||
    !parsed.approvals.every(queueRow)
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const rows = parsed.approvals as NativeWorkflowApprovalQueueRow[];
  if (
    rows.some((row) => row.company_id !== actor.companyId) ||
    !unique(rows.map((row) => row.id)) ||
    !unique(rows.map((row) => `${row.record_type}:${row.record_id}`)) ||
    !nonIncreasing(rows.map((row) => row.created_at))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  return { evaluatedAt: parsed.evaluatedAt, approvals: rows };
}

function evidence(
  value: unknown,
  actor: NativeWorkflowEvidenceActor,
  kind: NormalizedKind,
  recordId: string,
): NativeWorkflowApprovalEvidenceWorkspace {
  if (!exact(value, ['evaluatedAt', 'approval'])) return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const parsed = value as { evaluatedAt: unknown; approval: unknown };
  if (!integer(parsed.evaluatedAt)) return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  if (parsed.approval === null)
    return { evaluatedAt: parsed.evaluatedAt, approval: null };
  if (
    !exact(parsed.approval, ['instance', 'steps', 'history', 'receipts', 'audit', 'outbox'])
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const approval = parsed.approval as Row;
  if (!instance(approval.instance, actor, kind, recordId))
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const instanceId = approval.instance.id;
  if (
    !Array.isArray(approval.steps) ||
    approval.steps.length > 64 ||
    !approval.steps.every((value) => step(value, instanceId))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const steps = approval.steps as NativeWorkflowApprovalStep[];
  if (
    !unique(steps.map((value) => value.id)) ||
    !unique(steps.map((value) => String(value.ordinal))) ||
    !strictlyIncreasing(steps.map((value) => value.ordinal))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');

  const commandIds = new Set<string>();
  if (
    !Array.isArray(approval.history) ||
    approval.history.length > 512 ||
    !approval.history.every((value) => decision(value, instanceId, commandIds))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const history = approval.history as NativeWorkflowApprovalDecision[];
  if (
    !unique(history.map((value) => value.id)) ||
    !strictlyIncreasing(history.map((value) => value.created_at)) ||
    commandIds.size > 512
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');

  if (
    !Array.isArray(approval.receipts) ||
    approval.receipts.length > 512 ||
    !approval.receipts.every((value) => receipt(value, recordId, commandIds))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const receipts = approval.receipts as NativeWorkflowCommandReceipt[];
  if (
    !unique(receipts.map((value) => value.command_id)) ||
    !strictlyIncreasing(receipts.map((value) => value.created_at))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');

  if (
    !Array.isArray(approval.audit) ||
    approval.audit.length > 1_024 ||
    !approval.audit.every((value) => auditEvent(value, recordId, commandIds))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const audit = approval.audit as NativeWorkflowAuditEvent[];
  if (
    !unique(audit.map((value) => value.id)) ||
    !strictlyIncreasing(audit.map((value) => value.created_at))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');

  if (
    !Array.isArray(approval.outbox) ||
    approval.outbox.length > 512 ||
    !approval.outbox.every((value) => outboxEvent(value, recordId, commandIds))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const outbox = approval.outbox as NativeWorkflowOutboxEvent[];
  if (
    !unique(outbox.map((value) => value.id)) ||
    !strictlyIncreasing(outbox.map((value) => value.created_at))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');

  return {
    evaluatedAt: parsed.evaluatedAt,
    approval: {
      instance: approval.instance as NativeWorkflowApprovalInstance,
      steps,
      history,
      receipts,
      audit,
      outbox,
    },
  };
}

type NormalizedKind = NativeWorkflowApprovalKind;
const normalizedKinds = kinds;

function context(
  value: unknown,
  actor: NativeWorkflowEvidenceActor,
): Row {
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
    ])
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  const parsed = value as Row;
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
    parsed.scopeKind !== actor.scopeKind ||
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length > 512 ||
    !parsed.permissions.every((item) => identifier(item)) ||
    parsed.permissions.length !== actor.permissions.length ||
    !parsed.permissions.every((item) => actor.permissions.includes(item as string))
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeWorkflowEvidenceConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_WORKFLOW_EVIDENCE_CONFIGURATION_INVALID');
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
    return failure('NATIVE_WORKFLOW_EVIDENCE_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_WORKFLOW_EVIDENCE_CONFIGURATION_INVALID');
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
      return failure('NATIVE_WORKFLOW_EVIDENCE_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeWorkflowEvidenceTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeWorkflowEvidenceActor,
  path: string,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(
      new NativeWorkflowEvidenceError('NATIVE_WORKFLOW_EVIDENCE_AUTHORITY_INVALID'),
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
        new NativeWorkflowEvidenceError('NATIVE_WORKFLOW_EVIDENCE_COOKIE_LIMIT'),
      );
    headers.set('cookie', cookie);
  }
  if (
    !identifier(actor.companyId) ||
    !identifier(actor.assignmentId) ||
    !integer(actor.authority.revision) ||
    !integer(actor.authority.assignmentVersion, 1)
  )
    return Promise.reject(
      new NativeWorkflowEvidenceError('NATIVE_WORKFLOW_EVIDENCE_REQUEST_INVALID'),
    );
  headers.set('x-erpx-context-company', actor.companyId);
  headers.set('x-erpx-context-assignment', actor.assignmentId);
  headers.set('x-erpx-context-revision', String(actor.authority.revision));
  headers.set(
    'x-erpx-context-assignment-version',
    String(actor.authority.assignmentVersion),
  );
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}${path}`, base);
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
        return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_LIMIT');
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
    return failure('NATIVE_WORKFLOW_EVIDENCE_UNAVAILABLE', response.status);
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
          if (text(value.error.message, 2_000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(
      code ?? 'NATIVE_WORKFLOW_EVIDENCE_UNAVAILABLE',
      response.status,
      message,
    );
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
  }
}

function normalizedKind(value: string): NormalizedKind {
  if (!normalizedKinds.has(value))
    return failure('NATIVE_WORKFLOW_EVIDENCE_REQUEST_INVALID');
  return value as NormalizedKind;
}

export function nativeWorkflowEvidenceAvailable(): boolean {
  return process.env.ERPX_NATIVE_WORKFLOW_CONFIG !== undefined;
}

export function nativeWorkflowEvidenceConfiguration(
  raw: string,
): NativeWorkflowEvidenceConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeWorkflowEvidenceError('NATIVE_WORKFLOW_EVIDENCE_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_WORKFLOW_EVIDENCE_CONFIGURATION_INVALID');
  configuration(value as NativeWorkflowEvidenceConfiguration);
  return value as unknown as NativeWorkflowEvidenceConfiguration;
}

export function nativeWorkflowEvidenceFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeWorkflowEvidenceError)) return undefined;
  if (error.code === 'NATIVE_WORKFLOW_EVIDENCE_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_WORKFLOW_EVIDENCE_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: error.code === 'NATIVE_WORKFLOW_EVIDENCE_UNAVAILABLE' ? 'NOT_FOUND' : error.code,
      message: error.message,
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_WORKFLOW_EVIDENCE_UNAVAILABLE',
      message: 'The native Workflow approval evidence service is unavailable.',
    };
  if (error.code === 'NATIVE_WORKFLOW_EVIDENCE_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_WORKFLOW_EVIDENCE_UNAVAILABLE',
      message: 'The native Workflow approval evidence reader is unavailable.',
    };
  if (error.code === 'NATIVE_WORKFLOW_EVIDENCE_REQUEST_INVALID')
    return {
      status: 422,
      code: 'APPROVAL_EVIDENCE_REQUEST_INVALID',
      message: 'Review the approval evidence request.',
    };
  if (
    error.code === 'NATIVE_WORKFLOW_EVIDENCE_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_WORKFLOW_EVIDENCE_COOKIE_LIMIT' ||
    error.code === 'NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID' ||
    error.code === 'NATIVE_WORKFLOW_EVIDENCE_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_WORKFLOW_EVIDENCE_UNAVAILABLE',
      message: 'The native Workflow approval evidence reader is unavailable.',
    };
  return undefined;
}

export function createNativeWorkflowEvidenceClient(
  raw: NativeWorkflowEvidenceConfiguration,
  transport: NativeWorkflowEvidenceTransport = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeWorkflowEvidenceActor,
    path: string,
    validate: (value: unknown) => unknown,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.permissions.includes('organization.manage'))
      return failure('NATIVE_WORKFLOW_EVIDENCE_CONTEXT_CHANGED');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      actor,
      path,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeWorkflowEvidenceError) throw error;
      return failure('NATIVE_WORKFLOW_EVIDENCE_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID');
    context(value.context, actor);
    return validate(value.data);
  };
  return {
    queue: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeWorkflowEvidenceActor,
      signal?: AbortSignal,
    ) =>
      run(
        incoming,
        actor,
        '/workflow/approvals',
        (value) => queue(value, actor),
        signal,
      ) as Promise<NativeWorkflowApprovalQueue>,
    evidence: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeWorkflowEvidenceActor,
      kind: string,
      recordId: string,
      signal?: AbortSignal,
    ) => {
      const parsedKind = normalizedKind(kind);
      if (!identifier(recordId) || recordId.includes('/') || recordId.includes('\\'))
        return Promise.reject(
          new NativeWorkflowEvidenceError('NATIVE_WORKFLOW_EVIDENCE_REQUEST_INVALID'),
        );
      return run(
        incoming,
        actor,
        `/workflow/approvals/${parsedKind}/${encodeURIComponent(recordId)}/evidence`,
        (value) => evidence(value, actor, parsedKind, recordId),
        signal,
      ) as Promise<NativeWorkflowApprovalEvidenceWorkspace>;
    },
  };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
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
