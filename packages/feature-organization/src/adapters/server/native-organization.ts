import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type { OrganizationAccessData } from '../../lib/organization-access';
import type {
  OrganizationNode,
  OrganizationNodeChange,
  OrganizationOverview,
  OrganizationPosition,
} from '../../lib/organization-domain';
import type {
  PositionMoveInput,
  PositionMovePreview,
  PositionMoveRecord,
} from '@erpx/feature-people_payroll/lib/position-moves';

/** Rendering input from the retained route authorization, never a grant. */
export type NativeOrganizationPageContext = {
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

export type NativeOrganizationConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
export type NativeOrganizationPosition = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  title: string;
  level_id: string;
  level_number: number;
  reports_to_position_id: string | null;
  requirements: string;
  is_ceo: number;
  roster_required: number;
  active: number;
  version: number;
};
export type NativeOrganizationLevel = { id: string; name: string; level_number: number };
export type NativeOrganizationTemplate = {
  id: string;
  name: string;
  version: number;
  positions: Row[];
};
export type NativeOrganizationWorkspace = {
  nodes: Row[];
  positions: NativeOrganizationPosition[];
  levels: NativeOrganizationLevel[];
  templates: NativeOrganizationTemplate[];
  nodeChanges: Row[];
  changeQueue: Row;
  authorizedChannelNodes: string[];
  channels: Row[];
  actorId: string;
  canManage: boolean;
  canManageUnits: boolean;
  canManageSpending: boolean;
};

export class NativeOrganizationError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native Organization request could not be completed.');
    this.name = 'NativeOrganizationError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number): never => {
  throw new NativeOrganizationError(code, status);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);
const nullableIdentifier = (value: unknown): value is string | null => value === null || identifier(value);
const flag = (value: unknown): value is number => value === 0 || value === 1;
const unique = (values: readonly string[]): boolean => new Set(values).size === values.length;

function position(value: unknown): value is NativeOrganizationPosition {
  return (
    record(value) &&
    Object.keys(value).length === 13 &&
    identifier(value.id) &&
    identifier(value.company_id) &&
    nullableIdentifier(value.branch_id) &&
    nullableIdentifier(value.department_id) &&
    text(value.title, 200) &&
    identifier(value.level_id) &&
    integer(value.level_number, 1) &&
    nullableIdentifier(value.reports_to_position_id) &&
    text(value.requirements, 4000) &&
    flag(value.is_ceo) &&
    flag(value.roster_required) &&
    flag(value.active) &&
    integer(value.version, 1)
  );
}

function level(value: unknown): value is NativeOrganizationLevel {
  return (
    record(value) &&
    Object.keys(value).length === 3 &&
    identifier(value.id) &&
    text(value.name, 120) &&
    integer(value.level_number, 1)
  );
}

function template(value: unknown): value is NativeOrganizationTemplate {
  return (
    record(value) &&
    Object.keys(value).length === 4 &&
    identifier(value.id) &&
    text(value.name, 120) &&
    integer(value.version, 1) &&
    Array.isArray(value.positions) &&
    value.positions.length <= 100 &&
    value.positions.every(record)
  );
}

function workspace(value: unknown): NativeOrganizationWorkspace {
  if (
    !exact(value, ['data']) ||
    !exact(value.data, [
      'nodes',
      'positions',
      'levels',
      'templates',
      'nodeChanges',
      'changeQueue',
      'authorizedChannelNodes',
      'channels',
      'actorId',
      'canManage',
      'canManageUnits',
      'canManageSpending',
    ])
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const data = value.data;
  if (
    !Array.isArray(data.nodes) ||
    data.nodes.length > 4000 ||
    !data.nodes.every(
      (node) =>
        record(node) &&
        identifier(node.id) &&
        text(node.kind, 40) &&
        text(node.slug, 80) &&
        text(node.display_name, 120) &&
        optionalText(node.parent_id, 200) &&
        text(node.company_id, 200) &&
        optionalText(node.branch_id, 200) &&
        ['active', 'suspended', 'archived'].includes(String(node.status)) &&
        integer(node.hierarchy_revision, 1),
    ) ||
    !Array.isArray(data.positions) ||
    data.positions.length > 4000 ||
    !data.positions.every(position) ||
    !Array.isArray(data.levels) ||
    data.levels.length > 100 ||
    !data.levels.every(level) ||
    !Array.isArray(data.templates) ||
    data.templates.length > 500 ||
    !data.templates.every(template) ||
    !Array.isArray(data.nodeChanges) ||
    data.nodeChanges.length > 1000 ||
    !data.nodeChanges.every(
      (change) =>
        record(change) &&
        identifier(change.id) &&
        optionalText(change.target_node_id, 200) &&
        ['create', 'rename', 'suspend', 'reactivate', 'archive', 'relocate'].includes(String(change.action)) &&
        ['branch', 'department'].includes(String(change.node_kind)) &&
        optionalText(change.slug, 80) &&
        text(change.display_name, 120) &&
        optionalText(change.branch_id, 200) &&
        optionalText(change.source_branch_id, 200) &&
        integer(change.version, 1) &&
        ['draft', 'submitted', 'applied', 'rejected', 'cancelled'].includes(String(change.state)) &&
        typeof change.canReview === 'boolean' &&
        Array.isArray(change.events),
    ) ||
    !exact(data.changeQueue, ['recordId', 'state', 'query', 'page', 'pageSize', 'total', 'pages']) ||
    !Array.isArray(data.authorizedChannelNodes) ||
    !data.authorizedChannelNodes.every((value) => identifier(value)) ||
    !Array.isArray(data.channels) ||
    !data.channels.every(
      (channel) =>
        record(channel) &&
        identifier(channel.node_id) &&
        identifier(channel.room_id) &&
        text(channel.name, 120) &&
        text(channel.kind, 40) &&
        text(channel.status, 40),
    ) ||
    !identifier(data.actorId) ||
    typeof data.canManage !== 'boolean' ||
    typeof data.canManageUnits !== 'boolean' ||
    typeof data.canManageSpending !== 'boolean'
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return {
    nodes: data.nodes as Row[],
    positions: data.positions as NativeOrganizationPosition[],
    levels: data.levels as NativeOrganizationLevel[],
    templates: data.templates as NativeOrganizationTemplate[],
    nodeChanges: data.nodeChanges as Row[],
    changeQueue: data.changeQueue as Row,
    authorizedChannelNodes: data.authorizedChannelNodes as string[],
    channels: data.channels as Row[],
    actorId: data.actorId as string,
    canManage: data.canManage as boolean,
    canManageUnits: data.canManageUnits as boolean,
    canManageSpending: data.canManageSpending as boolean,
  };
}

const accessPersonKeys = [
  'person_id',
  'user_id',
  'legal_name',
  'email',
  'engagement_id',
  'engagement_version',
  'person_version',
  'company_id',
  'branch_id',
  'department_id',
  'position_id',
  'manager_person_id',
  'level_id',
  'level_number',
  'level_name',
  'status',
  'person_status',
  'start_date',
  'end_date',
  'job_title',
  'issues',
] as const;

function accessPerson(value: unknown): value is OrganizationAccessData['people'][number] {
  return (
    exact(value, accessPersonKeys) &&
    identifier(value.person_id) &&
    nullableIdentifier(value.user_id) &&
    text(value.legal_name, 300) &&
    optionalText(value.email, 320) &&
    identifier(value.engagement_id) &&
    integer(value.engagement_version, 1) &&
    integer(value.person_version, 1) &&
    identifier(value.company_id) &&
    nullableIdentifier(value.branch_id) &&
    nullableIdentifier(value.department_id) &&
    nullableIdentifier(value.position_id) &&
    nullableIdentifier(value.manager_person_id) &&
    nullableIdentifier(value.level_id) &&
    (value.level_number === null || integer(value.level_number, 1)) &&
    optionalText(value.level_name, 120) &&
    text(value.status, 40) &&
    text(value.person_status, 40) &&
    text(value.start_date, 20) &&
    optionalText(value.end_date, 20) &&
    text(value.job_title, 200) &&
    Array.isArray(value.issues) &&
    value.issues.length <= 20 &&
    value.issues.every((issue) => text(issue, 400))
  );
}

function accessPosition(value: unknown): value is OrganizationAccessData['positions'][number] {
  if (!record(value)) return false;
  const keys = Object.keys(value);
  if (keys.length !== 12 && keys.length !== 13) return false;
  const required = [
    'id',
    'company_id',
    'branch_id',
    'department_id',
    'title',
    'level_id',
    'level_number',
    'reports_to_position_id',
    'is_ceo',
    'roster_required',
    'active',
    'version',
  ];
  return (
    required.every((key) => Object.hasOwn(value, key)) &&
    identifier(value.id) &&
    identifier(value.company_id) &&
    nullableIdentifier(value.branch_id) &&
    nullableIdentifier(value.department_id) &&
    text(value.title, 200) &&
    identifier(value.level_id) &&
    integer(value.level_number, 1) &&
    nullableIdentifier(value.reports_to_position_id) &&
    (!Object.hasOwn(value, 'requirements') || optionalText(value.requirements, 4000)) &&
    flag(value.is_ceo) &&
    flag(value.roster_required) &&
    flag(value.active) &&
    integer(value.version, 1)
  );
}

function accessNode(value: unknown): value is OrganizationAccessData['nodes'][number] {
  return (
    exact(value, [
      'id',
      'kind',
      'slug',
      'display_name',
      'branch_id',
      'status',
      'hierarchy_revision',
    ]) &&
    identifier(value.id) &&
    text(value.kind, 40) &&
    text(value.slug, 80) &&
    text(value.display_name, 120) &&
    nullableIdentifier(value.branch_id) &&
    text(value.status, 20) &&
    integer(value.hierarchy_revision, 1)
  );
}

function accessLevel(value: unknown): value is OrganizationAccessData['levels'][number] {
  return (
    exact(value, ['id', 'name', 'level_number']) &&
    identifier(value.id) &&
    text(value.name, 120) &&
    integer(value.level_number, 1)
  );
}

/** Exact native access projection. The Rust contract is the retained
 * `OrganizationAccessData` wire shape; anything else fails closed. */
function accessData(value: unknown): OrganizationAccessData {
  if (
    !exact(value, ['data']) ||
    !exact(value.data, [
      'positions',
      'people',
      'nodes',
      'levels',
      'canManage',
      'canMovePeople',
      'actorId',
    ])
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const data = value.data;
  if (
    !Array.isArray(data.positions) ||
    data.positions.length > 4000 ||
    !data.positions.every(accessPosition) ||
    !Array.isArray(data.people) ||
    data.people.length > 4000 ||
    !data.people.every(accessPerson) ||
    !Array.isArray(data.nodes) ||
    data.nodes.length > 4000 ||
    !data.nodes.every(accessNode) ||
    !Array.isArray(data.levels) ||
    data.levels.length > 100 ||
    !data.levels.every(accessLevel) ||
    typeof data.canManage !== 'boolean' ||
    typeof data.canMovePeople !== 'boolean' ||
    !identifier(data.actorId)
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return data as unknown as OrganizationAccessData;
}

function movePlacement(value: unknown): boolean {
  return (
    exact(value, ['branchId', 'departmentId', 'levelId', 'reportsToPositionId']) &&
    nullableIdentifier(value.branchId) &&
    nullableIdentifier(value.departmentId) &&
    nullableIdentifier(value.levelId) &&
    nullableIdentifier(value.reportsToPositionId)
  );
}

function moveDefinition(value: unknown): value is PositionMoveInput {
  return (
    record(value) &&
    identifier(value.positionId) &&
    integer(value.expectedVersion, 1) &&
    ['department', 'level'].includes(String(value.targetKind)) &&
    identifier(value.targetId) &&
    integer(value.effectiveAt, 1) &&
    text(value.timezone, 80) &&
    Array.isArray(value.managerAssignments) &&
    value.managerAssignments.length <= 10000 &&
    value.managerAssignments.every(
      (item) =>
        record(item) &&
        identifier(item.engagementId) &&
        (item.managerPersonId === null || identifier(item.managerPersonId)),
    ) &&
    (!Object.hasOwn(value, 'reportsToPositionId') ||
      value.reportsToPositionId === null ||
      identifier(value.reportsToPositionId)) &&
    text(value.reason, 1000)
  );
}

function movePolicyChanges(value: unknown): boolean {
  if (!exact(value, ['added', 'removed', 'retained'])) return false;
  return ['added', 'removed', 'retained'].every((key) => {
    const items = value[key];
    return Array.isArray(items) && items.length <= 1000 && items.every(record);
  });
}

function movePreview(value: unknown): value is PositionMovePreview {
  return (
    exact(value, [
      'previewToken',
      'expiresAt',
      'ready',
      'blockers',
      'before',
      'after',
      'people',
      'policyChanges',
      'affectedCount',
      'effectiveAt',
      'timezone',
    ]) &&
    identifier(value.previewToken) &&
    integer(value.expiresAt, 1) &&
    typeof value.ready === 'boolean' &&
    Array.isArray(value.blockers) &&
    value.blockers.length <= 100 &&
    value.blockers.every((blocker) => text(blocker, 1000)) &&
    movePlacement(value.before) &&
    movePlacement(value.after) &&
    Array.isArray(value.people) &&
    value.people.length <= 10000 &&
    value.people.every(
      (person) =>
        record(person) &&
        identifier(person.engagementId) &&
        identifier(person.personId) &&
        text(person.name, 300) &&
        movePlacement(person.before) &&
        movePlacement(person.after) &&
        optionalText(person.leaveEffectiveFrom, 20) &&
        text(person.leaveNote, 2000),
    ) &&
    movePolicyChanges(value.policyChanges) &&
    integer(value.affectedCount, 0) &&
    integer(value.effectiveAt, 1) &&
    text(value.timezone, 80)
  );
}

function positionMoveRecord(value: unknown): value is PositionMoveRecord {
  return (
    exact(value, [
      'id',
      'company_id',
      'position_id',
      'status',
      'version',
      'effective_at',
      'definition',
      'preview',
      'created_by',
      'created_at',
      'updated_at',
      'applied_at',
      'last_error',
    ]) &&
    identifier(value.id) &&
    identifier(value.company_id) &&
    identifier(value.position_id) &&
    ['scheduled', 'applied', 'needs_review', 'cancelled'].includes(String(value.status)) &&
    integer(value.version, 1) &&
    integer(value.effective_at, 1) &&
    moveDefinition(value.definition) &&
    movePreview(value.preview) &&
    identifier(value.created_by) &&
    integer(value.created_at, 0) &&
    integer(value.updated_at, 0) &&
    (value.applied_at === null || integer(value.applied_at, 0)) &&
    (value.last_error === null || text(value.last_error, 4000))
  );
}

function positionMoves(value: unknown): PositionMoveRecord[] {
  if (
    !exact(value, ['data']) ||
    !Array.isArray(value.data) ||
    value.data.length > 100 ||
    !value.data.every(positionMoveRecord)
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return value.data;
}

function positionMovePreviewData(value: unknown): PositionMovePreview {
  if (!exact(value, ['data']) || !movePreview(value.data))
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return value.data;
}

function moveReceipt(value: unknown): NativePositionMoveReceipt {
  if (!exact(value, ['data']) || !record(value.data)) return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const data = value.data;
  const keys = Object.keys(data);
  if (
    !identifier(data.id) ||
    !integer(data.version, 1) ||
    !text(data.status, 40) ||
    !(
      keys.length === 3 ||
      (keys.length === 4 && Object.hasOwn(data, 'effectiveAt') && integer(data.effectiveAt, 1))
    ) ||
    !keys.every((key) => ['id', 'version', 'status', 'effectiveAt'].includes(key))
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return data as NativePositionMoveReceipt;
}

export type NativePositionMoveReceipt = Readonly<{
  id: string;
  version: number;
  status: string;
  effectiveAt?: number;
}>;

/** Compose the page GET from native Organization-owned facts and the retained
 * People projection. Roster/manager-grant/pay-group defaults still come from
 * their existing owner ports; no Organization SQL reads them directly. */
export function nativeOrganizationOverview(
  native: NativeOrganizationWorkspace,
  retained: Pick<
    OrganizationOverview,
    'people' | 'defaults' | 'monthlyPayGroups' | 'spendingTemplate'
  >,
): OrganizationOverview {
  return {
    positions: native.positions as unknown as OrganizationPosition[],
    people: retained.people,
    nodes: native.nodes as unknown as OrganizationNode[],
    levels: native.levels,
    templates: native.templates as unknown as OrganizationOverview['templates'],
    nodeChanges: native.nodeChanges as unknown as OrganizationNodeChange[],
    changeQueue: native.changeQueue as unknown as OrganizationOverview['changeQueue'],
    canManageUnits: native.canManageUnits,
    actorId: native.actorId,
    canManage: native.canManage,
    authorizedChannelNodes: native.authorizedChannelNodes,
    channels: native.channels as unknown as NonNullable<OrganizationOverview['channels']>,
    canManageSpending: native.canManageSpending,
    ...(retained.defaults === undefined ? {} : { defaults: retained.defaults }),
    ...(retained.monthlyPayGroups === undefined
      ? {}
      : { monthlyPayGroups: retained.monthlyPayGroups }),
    ...(retained.spendingTemplate === undefined
      ? {}
      : { spendingTemplate: retained.spendingTemplate }),
  };
}

/** Replace Organization-owned detail facts without dropping retained People
 * defaults, pay groups or spending template that the native workspace cannot
 * represent. */
export function nativeOrganizationDetail(
  native: OrganizationOverview,
  retained: OrganizationOverview,
  canManage: boolean,
): OrganizationOverview {
  return { ...retained, ...native, canManage };
}

/** Rendering input from the retained People page authorization, never a grant. */
export type NativeOrganizationAccessContext = Readonly<{
  userId: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
}>;

/** Compose the OrganizationAccessData used by the People/Organization page.
 * Native owns the tree, positions, levels and the Company-manage ceiling; the
 * People rows and the move-people ceiling stay retained because the native
 * workspace does not represent them. */
export function nativeOrganizationAccess(
  native: NativeOrganizationWorkspace,
  retained: OrganizationAccessData,
  actor: NativeOrganizationAccessContext,
): OrganizationAccessData {
  if (native.actorId !== actor.userId) return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const nodeIds = new Set<string>();
  const nodes = native.nodes.map((value) => {
    if (
      !record(value) ||
      !identifier(value.id) ||
      !text(value.kind, 40) ||
      !text(value.slug, 80) ||
      !text(value.display_name, 120) ||
      !nullableIdentifier(value.branch_id) ||
      !identifier(value.company_id) ||
      !['active', 'suspended', 'archived'].includes(String(value.status)) ||
      !integer(value.hierarchy_revision, 1)
    )
      return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
    if (
      value.company_id !== actor.companyId ||
      !['company', 'branch', 'department'].includes(value.kind) ||
      nodeIds.has(value.id) ||
      (actor.branchId !== null &&
        value.kind !== 'company' &&
        value.id !== actor.branchId &&
        value.branch_id !== actor.branchId) ||
      (actor.departmentId !== null && value.kind === 'department' && value.id !== actor.departmentId)
    )
      return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
    nodeIds.add(value.id);
    return {
      id: value.id,
      kind: value.kind,
      slug: value.slug,
      display_name: value.display_name,
      branch_id: value.branch_id,
      status: String(value.status),
      hierarchy_revision: value.hierarchy_revision,
    } satisfies OrganizationNode;
  });
  if (
    native.positions.some((value) => value.company_id !== actor.companyId) ||
    !unique(native.positions.map((value) => value.id)) ||
    !unique(native.levels.map((value) => value.id))
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const inScope = (value: NativeOrganizationPosition) =>
    value.active === 1 &&
    (actor.branchId === null || value.branch_id === actor.branchId) &&
    (actor.departmentId === null || value.department_id === actor.departmentId);
  return {
    positions: native.positions.filter(inScope) as unknown as OrganizationAccessData['positions'],
    people: retained.people,
    nodes,
    levels: native.levels,
    canManage: retained.canManage && native.canManage,
    canMovePeople: retained.canMovePeople,
    actorId: native.actorId,
  };
}

function commandData(value: unknown): Row {
  if (!exact(value, ['data']) || !record(value.data)) return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const data = value.data;
  if (!identifier(data.id) || !integer(data.version, 1) || !text(data.state, 40))
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return data;
}

function levelsData(value: unknown): Row {
  if (!exact(value, ['data']) || !exact(value.data, ['id']) || !identifier(value.data.id))
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return value.data;
}

function templateData(value: unknown): Row {
  if (
    !exact(value, ['data']) ||
    !exact(value.data, ['id', 'version']) ||
    !identifier(value.data.id) ||
    !integer(value.data.version, 1)
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return value.data;
}

function applyTemplateData(value: unknown): Row {
  if (
    !exact(value, ['data']) ||
    !exact(value.data, ['id', 'positionIds']) ||
    !identifier(value.data.id) ||
    !Array.isArray(value.data.positionIds) ||
    value.data.positionIds.length > 100 ||
    !value.data.positionIds.every((id) => identifier(id))
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return value.data;
}

function previewData(value: unknown): Row {
  if (!exact(value, ['data']) || !record(value.data)) return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const data = value.data;
  if (!identifier(data.previewSnapshot) || !record(data.impact))
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return data;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(configuration: NativeOrganizationConfiguration) {
  let base: URL;
  try {
    base = new URL(configuration.privateBaseUrl);
  } catch {
    return failure('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
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
    configuration.privateBaseUrl !== base.origin ||
    !integer(configuration.timeoutMs, 250) ||
    configuration.timeoutMs > 30000 ||
    configuration.holdings.length < 1 ||
    configuration.holdings.length > 64
  )
    return failure('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of configuration.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: configuration.timeoutMs };
}

type Transport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

function call(
  transport: Transport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeOrganizationPageContext,
  holdingSlug: string,
  section: 'organization' | 'access',
  path: string,
  body: string | undefined,
  idempotencyKey?: string,
  origin?: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug],
    ['x-erpx-workspace-department', actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined],
  ] as const) {
    if (slug !== undefined) {
      if (!/^[a-z0-9_-]+$/.test(slug)) return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
      headers.set(key, slug);
    }
  }
  if (actor.authority) {
    headers.set('x-erpx-context-company', actor.companyId);
    headers.set('x-erpx-context-assignment', actor.assignmentId);
    headers.set('x-erpx-context-revision', String(actor.authority.revision));
    headers.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
  }
  if (body !== undefined) {
    if (new TextEncoder().encode(body).length > 64 * 1024)
      return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    if (idempotencyKey !== undefined) {
      if (!/^[A-Za-z0-9:_.-]{8,180}$/.test(idempotencyKey))
        return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_COMMAND_KEY_INVALID'));
      headers.set('idempotency-key', idempotencyKey);
    }
    if (origin) {
      let originUrl: URL;
      try {
        originUrl = new URL(origin);
      } catch {
        return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
      }
      if (origin !== originUrl.origin) return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
      headers.set('origin', origin);
    }
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/people/${section}${path}`,
    base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, 30000);
  return transport(body === undefined ? 'GET' : 'POST', url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_ORGANIZATION_UNAVAILABLE', response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > 2 * 1024 * 1024) {
        await reader.cancel();
        return failure('NATIVE_ORGANIZATION_RESPONSE_LIMIT');
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
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  }
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
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
      {
        method,
        agent,
        headers: {
          ...Object.fromEntries(headers),
          ...(body === undefined ? {} : { 'content-length': String(new TextEncoder().encode(body).length) }),
        },
        signal,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          if (status !== 200) {
            incoming.destroy();
            resolve(new Response(null, { status: status >= 200 && status <= 599 ? status : 502 }));
            return;
          }
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status,
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
    if (body !== undefined) outgoing.write(body);
    outgoing.end();
  });
}

export function createNativeOrganizationClient(
  raw: NativeOrganizationConfiguration,
  transport: Transport = directHttp,
) {
  const config = configuration(raw);
  return {
    async workspace(
      incoming: Pick<Headers, 'get'>,
      actor: NativeOrganizationPageContext,
      query: string,
      signal?: AbortSignal,
    ): Promise<NativeOrganizationWorkspace> {
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        actor.holdingSlug,
        'organization',
        `?${query}`,
        undefined,
        undefined,
        undefined,
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeOrganizationError) throw error;
        return failure('NATIVE_ORGANIZATION_UNAVAILABLE');
      });
      return workspace(await boundedJson(response));
    },
    async change(
      incoming: Pick<Headers, 'get'>,
      actor: NativeOrganizationPageContext,
      input: {
        path: string;
        body: unknown;
        idempotencyKey: string;
        origin?: string;
        preview?: boolean;
        result?: 'command' | 'levels' | 'template' | 'apply';
      },
    ): Promise<Row> {
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        actor.holdingSlug,
        'organization',
        input.path,
        JSON.stringify(input.body),
        input.idempotencyKey,
        input.origin,
      ).catch((error: unknown) => {
        if (error instanceof NativeOrganizationError) throw error;
        return failure('NATIVE_ORGANIZATION_UNAVAILABLE');
      });
      const value = await boundedJson(response);
      if (input.preview) return previewData(value);
      if (input.result === 'levels') return levelsData(value);
      if (input.result === 'template') return templateData(value);
      if (input.result === 'apply') return applyTemplateData(value);
      return commandData(value);
    },
    async access(
      incoming: Pick<Headers, 'get'>,
      actor: NativeOrganizationPageContext,
      signal?: AbortSignal,
    ): Promise<OrganizationAccessData> {
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        actor.holdingSlug,
        'access',
        '',
        undefined,
        undefined,
        undefined,
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeOrganizationError) throw error;
        return failure('NATIVE_ORGANIZATION_UNAVAILABLE');
      });
      return accessData(await boundedJson(response));
    },
    async positionMoves(
      incoming: Pick<Headers, 'get'>,
      actor: NativeOrganizationPageContext,
      id?: string,
      signal?: AbortSignal,
    ): Promise<PositionMoveRecord[] | PositionMoveRecord> {
      if (id !== undefined && !identifier(id))
        return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
      const path = `/position-moves${id === undefined ? '' : `/${encodeURIComponent(id)}`}`;
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        actor.holdingSlug,
        'organization',
        path,
        undefined,
        undefined,
        undefined,
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeOrganizationError) throw error;
        return failure('NATIVE_ORGANIZATION_UNAVAILABLE');
      });
      const rows = positionMoves(await boundedJson(response));
      if (id === undefined) return rows;
      if (rows.length !== 1) return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
      return rows[0]!;
    },
    async previewPositionMove(
      incoming: Pick<Headers, 'get'>,
      actor: NativeOrganizationPageContext,
      body: unknown,
      signal?: AbortSignal,
    ): Promise<PositionMovePreview> {
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        actor.holdingSlug,
        'organization',
        '/position-moves/preview',
        JSON.stringify(body),
        undefined,
        undefined,
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeOrganizationError) throw error;
        return failure('NATIVE_ORGANIZATION_UNAVAILABLE');
      });
      return positionMovePreviewData(await boundedJson(response));
    },
    async submitPositionMove(
      incoming: Pick<Headers, 'get'>,
      actor: NativeOrganizationPageContext,
      input: { body: unknown; idempotencyKey: string; origin?: string },
    ): Promise<NativePositionMoveReceipt> {
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        actor.holdingSlug,
        'organization',
        '/position-moves/submit',
        JSON.stringify(input.body),
        input.idempotencyKey,
        input.origin,
      ).catch((error: unknown) => {
        if (error instanceof NativeOrganizationError) throw error;
        return failure('NATIVE_ORGANIZATION_UNAVAILABLE');
      });
      return moveReceipt(await boundedJson(response));
    },
    async cancelPositionMove(
      incoming: Pick<Headers, 'get'>,
      actor: NativeOrganizationPageContext,
      input: {
        id: string;
        expectedVersion: number;
        idempotencyKey: string;
        origin?: string;
      },
    ): Promise<NativePositionMoveReceipt> {
      if (!identifier(input.id) || !integer(input.expectedVersion, 1))
        return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        actor.holdingSlug,
        'organization',
        `/position-moves/${encodeURIComponent(input.id)}/cancel`,
        JSON.stringify({ expectedVersion: input.expectedVersion }),
        input.idempotencyKey,
        input.origin,
      ).catch((error: unknown) => {
        if (error instanceof NativeOrganizationError) throw error;
        return failure('NATIVE_ORGANIZATION_UNAVAILABLE');
      });
      return moveReceipt(await boundedJson(response));
    },
  };
}

export function nativeOrganizationConfiguration(raw: string): NativeOrganizationConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
  }
  if (!record(value) || !text(value.privateBaseUrl) || !integer(value.timeoutMs) || !Array.isArray(value.holdings))
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) && text(holding.origin) && text(holding.holdingSlug) && text(holding.holdingId),
    )
  )
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
  return value as unknown as NativeOrganizationConfiguration;
}

export function nativeOrganizationFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeOrganizationError)) return undefined;
  if (error.code === 'NATIVE_ORGANIZATION_RESPONSE_INVALID' || error.code === 'NATIVE_ORGANIZATION_RESPONSE_LIMIT')
    return { status: 502, code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID', message: 'The organization workspace reply was invalid.' };
  if (error.code === 'NATIVE_ORGANIZATION_CONTEXT_CHANGED')
    return { status: 409, code: 'WORK_CONTEXT_CHANGED', message: 'Your work role changed. Reload and review before submitting.' };
  if (error.code === 'NATIVE_ORGANIZATION_AUTHORITY_INVALID')
    return { status: 403, code: 'FORBIDDEN', message: 'This action is unavailable in your current role.' };
  if (error.code === 'NATIVE_ORGANIZATION_REQUEST_INVALID' || error.code === 'NATIVE_ORGANIZATION_COMMAND_KEY_INVALID' || error.code === 'NATIVE_ORGANIZATION_COOKIE_LIMIT')
    return { status: 422, code: 'NATIVE_ORGANIZATION_REQUEST_INVALID', message: 'Review the organization request and submit it again.' };
  if (error.code === 'NATIVE_ORGANIZATION_CONFIGURATION_INVALID')
    return { status: 503, code: 'NATIVE_ORGANIZATION_UNAVAILABLE', message: 'The organization workspace is unavailable.' };
  switch (error.status) {
    case 401:
      return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
    case 403:
      return { status: 403, code: 'FORBIDDEN', message: 'This action is unavailable in your current role.' };
    case 404:
      return { status: 404, code: 'NOT_FOUND', message: 'Organization change not found.' };
    case 409:
      return { status: 409, code: 'CONFLICT', message: 'The organization changed. Refresh and review again.' };
    case 422:
      return {
        status: 422,
        code: 'NATIVE_ORGANIZATION_REQUEST_INVALID',
        message: 'Review the organization request and submit it again.',
      };
    default:
      return { status: 503, code: 'NATIVE_ORGANIZATION_UNAVAILABLE', message: 'The organization workspace is unavailable.' };
  }
}
