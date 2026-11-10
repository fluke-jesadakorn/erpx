import 'server-only';
import { Agent, request } from 'node:http';
import { allowedPrivateLoopbackPort } from '@erpx/contracts/private-loopback';

/** Rendering input from the retained Manage ERPX authorization, never a grant. */
export type NativePlatformAdminActor = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  workAssignmentId: string;
  platformAssignmentId: string;
  platformRole: string;
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type NativePlatformAdminConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  production?: boolean;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

type Row = Record<string, unknown>;
export type NativePlatformResourceSlot = {
  id: string;
  write: string;
  read: string;
  files: string;
  beta?: boolean;
  internal?: boolean;
};
export type NativePlatformProvisioningRow = {
  id: string;
  holdingId: string;
  slug: string;
  displayName: string;
  writeBinding: string;
  readBinding: string;
  filesBinding: string;
  status: 'provisioning' | 'active' | 'failed' | 'suspended';
  requestedBy: string;
  reason: string;
  error: string | null;
  version: number;
  requestedAt: number;
  activatedAt: number | null;
  updatedAt: number;
};
export type NativePlatformProvisioningSnapshot = {
  rows: NativePlatformProvisioningRow[];
  availableSlots: NativePlatformResourceSlot[];
};
export type NativePlatformDeadLetter = {
  id: string;
  eventType: string;
  aggregateId: string;
  aggregateVersion: number;
  attempts: number;
  lastError: string | null;
  deadLetterAt: number;
};
export type NativePlatformMigrationState = {
  track: string;
  applied: number;
  pending: string[];
  latestApplied: string | null;
  error: string | null;
};
export type NativePlatformHoldingOperation = {
  id: string;
  slug: string;
  displayName: string;
  status: string;
  pendingEvents: number;
  deadLetterEvents: number;
  claimedEvents: number;
  oldestPendingAt: number | null;
  jobHeartbeatAt: number | null;
  error: string | null;
  deadLetters: NativePlatformDeadLetter[];
  migrations: NativePlatformMigrationState;
};
export type NativePlatformOperationsSummary = {
  generatedAt: number;
  actorRole: string;
  controlHealth: { service: string; heartbeatAt: number; details: string }[];
  holdings: NativePlatformHoldingOperation[];
  totals: {
    holdings: number;
    pendingEvents: number;
    deadLetterEvents: number;
    claimedEvents: number;
  };
};
export type NativePlatformOperationsSnapshot = {
  provisioning: NativePlatformProvisioningSnapshot;
  operations: NativePlatformOperationsSummary;
};
export type NativePlatformExecutiveGroup = {
  label: string;
  value: string;
};
export type NativePlatformExecutiveSourceReference = {
  dataset: string;
  revision: string;
  referencedAt: number;
};
export type NativePlatformExecutiveMetric = {
  id: 'failed-provisioning' | 'abnormal-platform-services';
  label: string;
  formula: string;
  dateBasis: string;
  total: string;
  rows: NativePlatformExecutiveGroup[];
  status: 'ready' | 'empty';
  sourceReferences: NativePlatformExecutiveSourceReference[];
};
export type NativePlatformExecutiveSnapshot = {
  scope: 'platform';
  referenceAt: number;
  filters: { from: string; to: string };
  sourceRevision: string;
  metrics: NativePlatformExecutiveMetric[];
};
export type NativePlatformExecutiveSourceRow =
  | { created_at: number; action: string }
  | { component: string; status: string; checked_at: number };
export type NativePlatformExecutiveSources = {
  id: string;
  name: string;
  metric: 'failed-provisioning' | 'abnormal-platform-services';
  filters: { from: string; to: string };
  rows: NativePlatformExecutiveSourceRow[];
  page: number;
  hasNext: boolean;
};
export type NativePlatformExecutiveQuery = {
  from?: string;
  to?: string;
  metric?: 'failed-provisioning' | 'abnormal-platform-services';
  group?: string;
  page?: number;
};
export type NativePlatformProvisioningResult = {
  id: string;
  holdingId: string;
  slug: string;
  displayName: string;
  status: string;
  version: number;
  activatedAt: number;
};
export type NativePlatformRecoveryResult = {
  eventId: string;
  holdingId: string;
  status: string;
  previousAttempts: number;
  previousError: string | null;
  recoveredAt: number;
};
export type NativePlatformMigrationResult = {
  runId: string;
  holdingId: string;
  track: 'holding-write';
  status: 'succeeded';
  appliedBefore: number;
  appliedAfter: number;
  pending: string[];
  finishedAt: number;
};

export class NativePlatformAdminError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Platform Administration request could not be completed.');
    this.name = 'NativePlatformAdminError';
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
  throw new NativePlatformAdminError(code, status, message);
};
const object = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
function fields(value: unknown, keys: readonly string[]): Row {
  if (
    !object(value) ||
    Object.keys(value).length !== keys.length ||
    !keys.every((key) => Object.hasOwn(value, key))
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return value;
}
function optionalFields(
  value: unknown,
  required: readonly string[],
  optional: readonly string[],
): Row {
  if (
    !object(value) ||
    !required.every((key) => Object.hasOwn(value, key)) ||
    !Object.keys(value).every((key) => required.includes(key) || optional.includes(key))
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return value;
}
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
const state = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-z][a-z0-9_-]{0,39}$/.test(value);
const binding = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Z][A-Z0-9_]{2,60}$/.test(value);
const slug = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-z][a-z0-9-]{2,39}$/.test(value);

function context(value: unknown, actor: NativePlatformAdminActor): Row {
  const row = fields(value, [
    'userId',
    'holdingId',
    'holdingSlug',
    'companyId',
    'branchId',
    'departmentId',
    'workAssignmentId',
    'platformAssignmentId',
    'platformRole',
    'assignmentVersion',
    'revision',
    'modules',
    'permissions',
    'expiresAt',
  ]);
  if (
    !identifier(row.userId) ||
    !identifier(row.holdingId) ||
    !identifier(row.holdingSlug) ||
    !identifier(row.companyId) ||
    !nullableIdentifier(row.branchId) ||
    !nullableIdentifier(row.departmentId) ||
    !identifier(row.workAssignmentId) ||
    !identifier(row.platformAssignmentId) ||
    !state(row.platformRole) ||
    !integer(row.assignmentVersion) ||
    !integer(row.revision) ||
    !Array.isArray(row.modules) ||
    !row.modules.every((item) => text(item, 80)) ||
    !Array.isArray(row.permissions) ||
    !row.permissions.every((item) => text(item, 120)) ||
    !integer(row.expiresAt) ||
    row.userId !== actor.userId ||
    row.holdingId !== actor.holdingId ||
    row.holdingSlug !== actor.holdingSlug ||
    row.companyId !== actor.companyId ||
    row.branchId !== actor.branchId ||
    row.departmentId !== actor.departmentId ||
    row.workAssignmentId !== actor.workAssignmentId ||
    row.platformAssignmentId !== actor.platformAssignmentId ||
    row.platformRole !== actor.platformRole
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return row;
}

function resourceSlot(value: unknown): NativePlatformResourceSlot {
  const row = optionalFields(value, ['id', 'write', 'read', 'files'], ['beta', 'internal']);
  if (
    !identifier(row.id) ||
    !/^[a-z0-9]{3,24}$/.test(row.id) ||
    !binding(row.write) ||
    !binding(row.read) ||
    !binding(row.files) ||
    (row.beta !== undefined && !boolean(row.beta)) ||
    (row.internal !== undefined && !boolean(row.internal))
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return row as NativePlatformResourceSlot;
}

function provisioningRow(value: unknown): NativePlatformProvisioningRow {
  const row = fields(value, [
    'id',
    'holdingId',
    'slug',
    'displayName',
    'writeBinding',
    'readBinding',
    'filesBinding',
    'status',
    'requestedBy',
    'reason',
    'error',
    'version',
    'requestedAt',
    'activatedAt',
    'updatedAt',
  ]);
  if (
    !identifier(row.id) ||
    !identifier(row.holdingId) ||
    !slug(row.slug) ||
    !text(row.displayName, 120) ||
    !binding(row.writeBinding) ||
    !binding(row.readBinding) ||
    !binding(row.filesBinding) ||
    !['provisioning', 'active', 'failed', 'suspended'].includes(String(row.status)) ||
    !identifier(row.requestedBy) ||
    !text(row.reason, 1000) ||
    !optionalText(row.error, 1000) ||
    !integer(row.version, 1) ||
    !integer(row.requestedAt) ||
    !(row.activatedAt === null || integer(row.activatedAt)) ||
    !integer(row.updatedAt)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return row as unknown as NativePlatformProvisioningRow;
}

function provisioning(value: unknown): NativePlatformProvisioningSnapshot {
  const row = fields(value, ['rows', 'availableSlots']);
  if (!Array.isArray(row.rows) || row.rows.length > 500 || !Array.isArray(row.availableSlots))
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return {
    rows: row.rows.map(provisioningRow),
    availableSlots: row.availableSlots.map(resourceSlot),
  };
}

function deadLetter(value: unknown): NativePlatformDeadLetter {
  const row = fields(value, [
    'id',
    'eventType',
    'aggregateId',
    'aggregateVersion',
    'attempts',
    'lastError',
    'deadLetterAt',
  ]);
  if (
    !identifier(row.id) ||
    !text(row.eventType, 200) ||
    !identifier(row.aggregateId) ||
    !integer(row.aggregateVersion) ||
    !integer(row.attempts) ||
    !optionalText(row.lastError, 64) ||
    !integer(row.deadLetterAt)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return row as NativePlatformDeadLetter;
}

function migrations(value: unknown): NativePlatformMigrationState {
  const row = fields(value, ['track', 'applied', 'pending', 'latestApplied', 'error']);
  if (
    row.track !== 'holding-write' ||
    !integer(row.applied) ||
    !Array.isArray(row.pending) ||
    !row.pending.every((item) => text(item, 240)) ||
    !optionalText(row.latestApplied, 240) ||
    !optionalText(row.error, 1000)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return row as NativePlatformMigrationState;
}

function holdingOperation(value: unknown): NativePlatformHoldingOperation {
  const row = fields(value, [
    'id',
    'slug',
    'displayName',
    'status',
    'pendingEvents',
    'deadLetterEvents',
    'claimedEvents',
    'oldestPendingAt',
    'jobHeartbeatAt',
    'error',
    'deadLetters',
    'migrations',
  ]);
  if (
    !identifier(row.id) ||
    !identifier(row.slug) ||
    !text(row.displayName, 120) ||
    !state(row.status) ||
    !integer(row.pendingEvents) ||
    !integer(row.deadLetterEvents) ||
    !integer(row.claimedEvents) ||
    !(row.oldestPendingAt === null || integer(row.oldestPendingAt)) ||
    !(row.jobHeartbeatAt === null || integer(row.jobHeartbeatAt)) ||
    !optionalText(row.error, 1000) ||
    !Array.isArray(row.deadLetters) ||
    !Array.isArray((row as Row).deadLetters)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return {
    ...(row as NativePlatformHoldingOperation),
    deadLetters: (row.deadLetters as unknown[]).map(deadLetter),
    migrations: migrations(row.migrations),
  };
}

function operations(value: unknown): NativePlatformOperationsSummary {
  const row = fields(value, ['generatedAt', 'actorRole', 'controlHealth', 'holdings', 'totals']);
  if (
    !integer(row.generatedAt) ||
    !state(row.actorRole) ||
    !Array.isArray(row.controlHealth) ||
    !Array.isArray(row.holdings) ||
    row.holdings.length > 200 ||
    !object(row.totals)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  const controlHealth = row.controlHealth.map((item) => {
    const health = fields(item, ['service', 'heartbeatAt', 'details']);
    if (!text(health.service, 120) || !integer(health.heartbeatAt) || !text(health.details, 4096))
      return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
    return health as NativePlatformOperationsSummary['controlHealth'][number];
  });
  const totals = fields(row.totals, [
    'holdings',
    'pendingEvents',
    'deadLetterEvents',
    'claimedEvents',
  ]);
  if (
    !integer(totals.holdings) ||
    !integer(totals.pendingEvents) ||
    !integer(totals.deadLetterEvents) ||
    !integer(totals.claimedEvents)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return {
    generatedAt: row.generatedAt,
    actorRole: row.actorRole,
    controlHealth,
    holdings: row.holdings.map(holdingOperation),
    totals: totals as NativePlatformOperationsSummary['totals'],
  };
}

function snapshot(value: unknown, actor: NativePlatformAdminActor): NativePlatformOperationsSnapshot {
  const row = fields(value, ['data', 'context']);
  context(row.context, actor);
  const data = fields(row.data, ['provisioning', 'operations']);
  return {
    provisioning: provisioning(data.provisioning),
    operations: operations(data.operations),
  };
}

function projection(
  value: unknown,
  actor: NativePlatformAdminActor,
): NativePlatformOperationsSummary {
  const row = fields(value, ['data', 'context']);
  context(row.context, actor);
  return operations(row.data);
}

const executiveMetricIds = ['failed-provisioning', 'abnormal-platform-services'] as const;
const executiveDate = (value: unknown): value is string =>
  typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value);
const executiveFilters = (value: unknown): { from: string; to: string } => {
  const row = fields(value, ['from', 'to']);
  if (!executiveDate(row.from) || !executiveDate(row.to))
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return row as { from: string; to: string };
};
const executiveGroup = (value: unknown): NativePlatformExecutiveGroup => {
  const row = fields(value, ['label', 'value']);
  if (
    !text(row.label, 250) ||
    typeof row.value !== 'string' ||
    !/^-?\d+$/.test(row.value)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return row as NativePlatformExecutiveGroup;
};
const executiveReference = (value: unknown): NativePlatformExecutiveSourceReference => {
  const row = fields(value, ['dataset', 'revision', 'referencedAt']);
  if (
    !text(row.dataset, 200) ||
    !/^[a-f0-9]{64}$/.test(String(row.revision)) ||
    !integer(row.referencedAt)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return row as NativePlatformExecutiveSourceReference;
};
const executiveMetric = (value: unknown): NativePlatformExecutiveMetric => {
  const row = fields(value, [
    'id',
    'label',
    'formula',
    'dateBasis',
    'total',
    'rows',
    'status',
    'sourceReferences',
  ]);
  if (
    !executiveMetricIds.includes(row.id as (typeof executiveMetricIds)[number]) ||
    !text(row.label, 250) ||
    !text(row.formula, 2000) ||
    !text(row.dateBasis, 500) ||
    typeof row.total !== 'string' ||
    !/^-?\d+$/.test(row.total) ||
    !Array.isArray(row.rows) ||
    row.rows.length > 10_000 ||
    !['ready', 'empty'].includes(String(row.status)) ||
    !Array.isArray(row.sourceReferences) ||
    row.sourceReferences.length < 1 ||
    row.sourceReferences.length > 8
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return {
    ...(row as unknown as NativePlatformExecutiveMetric),
    rows: row.rows.map(executiveGroup),
    sourceReferences: row.sourceReferences.map(executiveReference),
  };
};
const executiveSnapshot = (
  value: unknown,
  actor: NativePlatformAdminActor,
): NativePlatformExecutiveSnapshot => {
  const row = fields(value, ['data', 'context']);
  context(row.context, actor);
  const data = fields(row.data, [
    'scope',
    'referenceAt',
    'filters',
    'sourceRevision',
    'metrics',
  ]);
  const scope = data.scope;
  const referenceAt = data.referenceAt;
  const sourceRevision = data.sourceRevision;
  if (
    scope !== 'platform' ||
    !integer(referenceAt) ||
    typeof sourceRevision !== 'string' ||
    !/^[a-f0-9]{64}$/.test(sourceRevision) ||
    !Array.isArray(data.metrics) ||
    data.metrics.length !== 2
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  const metrics = data.metrics.map(executiveMetric);
  if (
    new Set(metrics.map((metric) => metric.id)).size !== 2 ||
    metrics.some((metric) => metric.sourceReferences.some((source) => source.revision !== sourceRevision))
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return {
    scope: 'platform',
    referenceAt,
    filters: executiveFilters(data.filters),
    sourceRevision,
    metrics,
  };
};
const executiveSources = (
  value: unknown,
  actor: NativePlatformAdminActor,
): NativePlatformExecutiveSources => {
  const row = fields(value, ['data', 'context']);
  context(row.context, actor);
  const data = fields(row.data, [
    'id',
    'name',
    'metric',
    'filters',
    'rows',
    'page',
    'hasNext',
  ]);
  if (
    !text(data.id, 250) ||
    !text(data.name, 250) ||
    !executiveMetricIds.includes(data.metric as (typeof executiveMetricIds)[number]) ||
    !Array.isArray(data.rows) ||
    data.rows.length > 30 ||
    !integer(data.page, 1) ||
    data.page > 100_000 ||
    !boolean(data.hasNext)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  const rows = data.rows.map((item) => {
    if (data.metric === 'failed-provisioning') {
      const row = fields(item, ['created_at', 'action']);
      if (!integer(row.created_at) || !text(row.action, 40))
        return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
      return row as NativePlatformExecutiveSourceRow;
    }
    const row = fields(item, ['component', 'status', 'checked_at']);
    if (!text(row.component, 200) || !text(row.status, 40) || !integer(row.checked_at))
      return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
    return row as NativePlatformExecutiveSourceRow;
  });
  return {
    id: data.id,
    name: data.name,
    metric: data.metric as NativePlatformExecutiveSources['metric'],
    filters: executiveFilters(data.filters),
    rows,
    page: data.page,
    hasNext: data.hasNext,
  };
};

function result(value: unknown, actor: NativePlatformAdminActor): NativePlatformProvisioningResult {
  const row = fields(value, ['data', 'context']);
  context(row.context, actor);
  const data = fields(row.data, [
    'id',
    'holdingId',
    'slug',
    'displayName',
    'status',
    'version',
    'activatedAt',
  ]);
  if (
    !identifier(data.id) ||
    !identifier(data.holdingId) ||
    !slug(data.slug) ||
    !text(data.displayName, 120) ||
    data.status !== 'active' ||
    !integer(data.version, 1) ||
    !integer(data.activatedAt)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return data as NativePlatformProvisioningResult;
}

function recovery(value: unknown, actor: NativePlatformAdminActor): NativePlatformRecoveryResult {
  const row = fields(value, ['data', 'context']);
  context(row.context, actor);
  const data = fields(row.data, [
    'eventId',
    'holdingId',
    'status',
    'previousAttempts',
    'previousError',
    'recoveredAt',
  ]);
  if (
    !identifier(data.eventId) ||
    !identifier(data.holdingId) ||
    data.status !== 'queued' ||
    !integer(data.previousAttempts, 1) ||
    !optionalText(data.previousError, 64) ||
    !integer(data.recoveredAt)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return data as NativePlatformRecoveryResult;
}

function migration(value: unknown, actor: NativePlatformAdminActor): NativePlatformMigrationResult {
  const row = fields(value, ['data', 'context']);
  context(row.context, actor);
  const data = fields(row.data, [
    'runId',
    'holdingId',
    'track',
    'status',
    'appliedBefore',
    'appliedAfter',
    'pending',
    'finishedAt',
  ]);
  if (
    !identifier(data.runId) ||
    !identifier(data.holdingId) ||
    data.track !== 'holding-write' ||
    data.status !== 'succeeded' ||
    !integer(data.appliedBefore) ||
    !integer(data.appliedAfter) ||
    data.appliedAfter < data.appliedBefore ||
    !Array.isArray(data.pending) ||
    !data.pending.every((item) => text(item, 240)) ||
    !integer(data.finishedAt)
  )
    return fail('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  return data as NativePlatformMigrationResult;
}

function configuration(raw: NativePlatformAdminConfiguration): NativePlatformAdminConfiguration {
  if (
    typeof raw?.privateBaseUrl !== 'string' ||
    (raw.production !== undefined && typeof raw.production !== 'boolean') ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID');
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
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID');
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
      throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== holding.origin ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug) ||
      ids.has(holding.holdingId)
    )
      throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID');
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
      { method, agent, headers: Object.fromEntries(headers), signal, maxHeaderSize: 8192 },
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
  if (!response.body) throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > 8 * 1024 * 1024)
        throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_RESPONSE_LIMIT');
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const text = new TextDecoder().decode(Buffer.concat(chunks.map((chunk) => Buffer.from(chunk))));
  try {
    return JSON.parse(text);
  } catch {
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID');
  }
}

function responseHeaders(
  incoming: Pick<Headers, 'get'>,
  actor: NativePlatformAdminActor,
  body: boolean,
): Headers {
  const host = incoming.get('host');
  if (!host) throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_AUTHORITY_INVALID');
  const headers = new Headers({
    host,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  const origin = incoming.get('origin');
  if (origin) headers.set('origin', origin);
  if (!actor.authority || !actor.iam) {
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONTEXT_CHANGED');
  }
  headers.set('x-erpx-context-company', actor.companyId);
  headers.set('x-erpx-context-assignment', actor.workAssignmentId);
  headers.set('x-erpx-context-revision', String(actor.authority.revision));
  headers.set(
    'x-erpx-context-assignment-version',
    String(actor.authority.assignmentVersion),
  );
  headers.set('x-erpx-platform-assignment', actor.platformAssignmentId);
  if (body) {
    headers.set('content-type', 'application/json');
    headers.set('x-erpx-method', 'POST');
    headers.set('x-erpx-request-method', 'POST');
  }
  return headers;
}

async function call(
  transport: Transport,
  config: NativePlatformAdminConfiguration,
  incoming: Pick<Headers, 'get'>,
  actor: NativePlatformAdminActor,
  path: string,
  body: string | undefined,
  idempotencyKey?: string,
  origin?: string,
): Promise<unknown> {
  const holding = config.holdings.find(
    (candidate) =>
      candidate.holdingSlug === actor.holdingSlug && candidate.holdingId === actor.holdingId,
  );
  if (!holding) throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONTEXT_CHANGED');
  const headers = responseHeaders(incoming, actor, body !== undefined);
  if (body !== undefined) {
    if (new TextEncoder().encode(body).length > 512 * 1024)
      throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
    if (!idempotencyKey || !/^[A-Za-z0-9:_.-]{8,180}$/.test(idempotencyKey))
      throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_COMMAND_KEY_INVALID');
    headers.set('idempotency-key', idempotencyKey);
    if (!origin) throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
    let originUrl: URL;
    try {
      originUrl = new URL(origin);
    } catch {
      throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
    }
    if (origin !== originUrl.origin || origin !== holding.origin)
      throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
    headers.set('origin', origin);
  }
  const url = new URL(`/api/v1/platform${path}`, config.privateBaseUrl);
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
      let code = 'NATIVE_PLATFORM_ADMIN_UNAVAILABLE';
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
      throw new NativePlatformAdminError(code, response.status, message);
    }
    return await boundedJson(response);
  } catch (error) {
    if (error instanceof NativePlatformAdminError) throw error;
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_UNAVAILABLE');
  } finally {
    clearTimeout(timer);
  }
}

export function nativePlatformAdminConfiguration(raw: string): NativePlatformAdminConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID');
  }
  if (!object(value)) throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID');
  return configuration(value as unknown as NativePlatformAdminConfiguration);
}

export function createNativePlatformAdminClient(
  raw: NativePlatformAdminConfiguration,
  transport: Transport = directHttp,
) {
  const config = configuration(raw);
  return {
    async operations(
      incoming: Pick<Headers, 'get'>,
      actor: NativePlatformAdminActor,
    ): Promise<NativePlatformOperationsSnapshot> {
      return snapshot(
        await call(transport, config, incoming, actor, '/operations', undefined),
        actor,
      );
    },
    async operationsProjection(
      incoming: Pick<Headers, 'get'>,
      actor: NativePlatformAdminActor,
    ): Promise<NativePlatformOperationsSummary> {
      return projection(
        await call(
          transport,
          config,
          incoming,
          actor,
          '/operations/projection',
          undefined,
        ),
        actor,
      );
    },
    async executive(
      incoming: Pick<Headers, 'get'>,
      actor: NativePlatformAdminActor,
      query: NativePlatformExecutiveQuery = {},
    ): Promise<NativePlatformExecutiveSnapshot> {
      const params = new URLSearchParams();
      if (query.metric !== undefined) {
        if (!executiveMetricIds.includes(query.metric))
          throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
        params.set('metric', query.metric);
        if (!query.group || !query.from || !query.to)
          throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
        params.set('group', query.group);
      }
      if (query.from !== undefined) params.set('from', query.from);
      if (query.to !== undefined) params.set('to', query.to);
      if (query.page !== undefined) {
        if (!Number.isSafeInteger(query.page) || query.page < 1 || query.page > 100_000)
          throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
        params.set('page', String(query.page));
      }
      if (query.metric === undefined && params.has('page'))
        throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
      return executiveSnapshot(
        await call(
          transport,
          config,
          incoming,
          actor,
          `/executive${params.size ? `?${params}` : ''}`,
          undefined,
        ),
        actor,
      );
    },
    async executiveSources(
      incoming: Pick<Headers, 'get'>,
      actor: NativePlatformAdminActor,
      query: Required<Pick<NativePlatformExecutiveQuery, 'metric' | 'group' | 'from' | 'to'>> &
        Pick<NativePlatformExecutiveQuery, 'page'>,
    ): Promise<NativePlatformExecutiveSources> {
      const params = new URLSearchParams({
        metric: query.metric,
        group: query.group,
        from: query.from,
        to: query.to,
      });
      if (query.page !== undefined) {
        if (!Number.isSafeInteger(query.page) || query.page < 1 || query.page > 100_000)
          throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
        params.set('page', String(query.page));
      }
      return executiveSources(
        await call(
          transport,
          config,
          incoming,
          actor,
          `/executive?${params}`,
          undefined,
        ),
        actor,
      );
    },
    async provision(
      incoming: Pick<Headers, 'get'>,
      actor: NativePlatformAdminActor,
      body: unknown,
      idempotencyKey: string,
    ): Promise<NativePlatformProvisioningResult> {
      if (!object(body)) throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
      return result(
        await call(
          transport,
          config,
          incoming,
          actor,
          '/operations/provision',
          JSON.stringify(body),
          idempotencyKey,
          incoming.get('origin') ?? undefined,
        ),
        actor,
      );
    },
    async recover(
      incoming: Pick<Headers, 'get'>,
      actor: NativePlatformAdminActor,
      body: unknown,
      idempotencyKey: string,
    ): Promise<NativePlatformRecoveryResult> {
      if (!object(body)) throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
      return recovery(
        await call(
          transport,
          config,
          incoming,
          actor,
          '/operations/recover',
          JSON.stringify(body),
          idempotencyKey,
          incoming.get('origin') ?? undefined,
        ),
        actor,
      );
    },
    async migrate(
      incoming: Pick<Headers, 'get'>,
      actor: NativePlatformAdminActor,
      body: unknown,
      idempotencyKey: string,
    ): Promise<NativePlatformMigrationResult> {
      if (!object(body)) throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID');
      return migration(
        await call(
          transport,
          config,
          incoming,
          actor,
          '/operations/migrate',
          JSON.stringify(body),
          idempotencyKey,
          incoming.get('origin') ?? undefined,
        ),
        actor,
      );
    },
  };
}

export function nativePlatformAdminFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePlatformAdminError)) return undefined;
  if (
    error.code === 'NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID' ||
    error.code === 'NATIVE_PLATFORM_ADMIN_RESPONSE_LIMIT'
  )
    return {
      status: 502,
      code: 'NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID',
      message: 'The Platform Administration reply was invalid.',
    };
  if (error.code === 'NATIVE_PLATFORM_ADMIN_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before submitting.',
    };
  if (error.code === 'NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_PLATFORM_ADMIN_UNAVAILABLE',
      message: 'Platform Administration native access is unavailable.',
    };
  if (
    error.code === 'NATIVE_PLATFORM_ADMIN_REQUEST_INVALID' ||
    error.code === 'NATIVE_PLATFORM_ADMIN_COMMAND_KEY_INVALID' ||
    error.code === 'NATIVE_PLATFORM_ADMIN_COOKIE_LIMIT'
  )
    return {
      status: 422,
      code: 'NATIVE_PLATFORM_ADMIN_REQUEST_INVALID',
      message: 'Review the Platform Administration request.',
    };
  if (error.code === 'NATIVE_PLATFORM_ADMIN_AUTHORITY_INVALID')
    return {
      status: 403,
      code: 'FORBIDDEN',
      message: 'The Platform Administration authority is unavailable.',
    };
  return {
    status: error.status && error.status >= 400 && error.status <= 599 ? error.status : 503,
    code: error.code,
    message: error.message,
  };
}
