import { Agent, request } from 'node:http';
import type { PmsWorkspace } from '@erpx/pms-server/public/pms';

/** Rendering input from the retained page authorization, never a grant. */
export type NativePmsPageContext = {
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

export type NativePmsConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativePmsBoardSection = 'rooms' | 'housekeeping';

export type NativePmsFrontDeskStatus =
  | ''
  | 'arrivals'
  | 'departures'
  | 'reserved'
  | 'in_house'
  | 'checked_out'
  | 'cancelled'
  | 'no_show'
  | 'completed';

export type NativePmsReadInput = Readonly<{
  holdingSlug: string;
  section: NativePmsBoardSection;
  property?: string;
  status?: string;
  q?: string;
  building?: string;
  floor?: string;
  typeId?: string;
  page?: number;
  selectedId?: string;
  from?: string;
  to?: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativePmsFrontDeskInput = Readonly<{
  holdingSlug: string;
  property?: string;
  status?: NativePmsFrontDeskStatus;
  q?: string;
  page?: number;
  selectedId?: string;
  from?: string;
  to?: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativePmsTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export type NativePmsGuestAccountCommandInput = Readonly<{
  holdingSlug: string;
  body: Record<string, unknown>;
  key: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativePmsImportCommandInput = Readonly<{
  holdingSlug: string;
  command: string;
  facts: Record<string, unknown>;
  key: string;
  userId: string;
  companyId: string;
  assignmentId: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

/** Bounded report views owned by pms_report_router (range summary / entry ledger). */
export type NativePmsReportSelection =
  | Readonly<{
      view: 'range';
      property: string;
      from: string;
      to: string;
      asOf?: string;
      compareAsOf?: string;
    }>
  | Readonly<{ view: 'entries'; property: string; date: string; page: number }>;

export type NativePmsReportInput = NativePmsReportSelection &
  Readonly<{
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

/** Owner read facts returned by the native report projection (PMS-owned only). */
export type NativePmsReportDay = {
  date: string;
  capacity: number;
  sold: number;
  roomRevenue: number;
  netRevenue: number;
  tax: number;
  refunds: number;
  cancellations: number;
  noShows: number;
};

export type NativePmsReportTotals = {
  capacity: number;
  sold: number;
  roomRevenue: number;
  netRevenue: number;
  tax: number;
  refunds: number;
  cancellations: number;
  noShows: number;
  occupancy: number;
  adr: number;
  revpar: number;
};

/** One booking-observation point from the retained pace window. */
export type NativePmsPacePoint = {
  asOf: string;
  coverageSince: string | null;
  covered: boolean;
  reservations: number;
  roomNights: number;
  quotedTotal: number;
};

export type NativePmsNetPickup = {
  reservations: number;
  roomNights: number;
  quotedTotal: number;
};

export type NativePmsReportPace = {
  current: NativePmsPacePoint | null;
  previous: NativePmsPacePoint | null;
  netPickup: NativePmsNetPickup | null;
};

export type NativePmsChannelReport = {
  source: string;
  reservations: number;
  roomNights: number;
  quotedTotal: number;
  cancelled: number;
  noShows: number;
  recentPickup: number;
  unknownCreation: number;
};

/** Finance-backed balances from the `pms_reporting_facts` port. */
export type NativePmsFinanceBalances = {
  receivable: number;
  advance: number;
  deposit: number;
  refundPayable: number;
  unallocated: number;
};

export type NativePmsReportData = {
  property: { id: string; name: string; branchId: string; state: string; businessDate: string };
  businessDate: string;
  currency: string;
  from: string;
  to: string;
  days: NativePmsReportDay[];
  totals: NativePmsReportTotals;
  flow: { arrivals: number; departures: number; inHouse: number };
  balances: {
    source: string;
    due: number;
    advance: number;
    deposit: number;
    totalAccounts: number;
    openAccounts: number;
    creditAccounts: number;
    accountsWithDue: number;
  };
  nightAudit: {
    businessDate: string;
    nightState: string | null;
    jobState: string | null;
    ready: boolean;
    blockers: string[];
  };
  asOf: string;
  pace: NativePmsReportPace;
  channels: NativePmsChannelReport[];
  financeBalances: NativePmsFinanceBalances;
};

export type NativePmsReportEntriesData = {
  date: string;
  page: number;
  rows: { id: string; data: Record<string, unknown> }[];
  hasNext: boolean;
};

/** Flatten the retained workspace command body ({operation,propertyId,
 * expectedVersion,input}) into the native guest-account body. The guest-save
 * contact command is intentionally not mapped; it is not a guest account. */
export function nativePmsGuestAccountBody(body: {
  operation: string;
  propertyId?: string;
  expectedVersion?: number;
  input?: unknown;
}): Record<string, unknown> {
  const input =
    body.input && typeof body.input === 'object' && !Array.isArray(body.input)
      ? { ...(body.input as Record<string, unknown>) }
      : {};
  if (
    body.operation === 'guest_account.transfer' &&
    typeof input.entryId !== 'string' &&
    typeof input.id === 'string'
  )
    input.entryId = input.id;
  return {
    ...input,
    operation: body.operation,
    ...(body.propertyId !== undefined ? { propertyId: body.propertyId } : {}),
    ...(body.expectedVersion !== undefined ? { expectedVersion: body.expectedVersion } : {}),
  };
}

export class NativePmsError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Hotel request could not be completed.');
    this.name = 'NativePmsError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativePmsError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown, max = 200): value is string =>
  text(value, max) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const kinds = new Set([
  'room_type',
  'room',
  'rate',
  'guest',
  'reservation',
  'stay',
  'guest_account',
  'entry',
  'task',
  'block',
  'shift',
  'refund',
  'item',
  'movement',
  'lost',
  'credit',
  'night',
]);

const propertySummary = (value: unknown) =>
  exact(value, ['id', 'name', 'branchId', 'state', 'businessDate']) &&
  identifier(value.id) &&
  text(value.name, 400) &&
  identifier(value.branchId) &&
  text(value.state, 40) &&
  text(value.businessDate, 10);

const property = (value: unknown) =>
  exact(value, [
    'id',
    'companyId',
    'branchId',
    'name',
    'businessDate',
    'state',
    'version',
    'config',
  ]) &&
  identifier(value.id) &&
  identifier(value.companyId) &&
  identifier(value.branchId) &&
  text(value.name, 400) &&
  text(value.businessDate, 10) &&
  text(value.state, 40) &&
  integer(value.version, 1) &&
  record(value.config);

const boardRecord = (value: unknown) =>
  exact(value, ['id', 'kind', 'version', 'updatedAt', 'data']) &&
  identifier(value.id) &&
  kinds.has(String(value.kind)) &&
  integer(value.version, 1) &&
  integer(value.updatedAt) &&
  record(value.data);

const roomSummary = (value: unknown) =>
  exact(value, ['total', 'active', 'dirty', 'clean', 'inspected', 'dnd', 'occupied']) &&
  ['total', 'active', 'dirty', 'clean', 'inspected', 'dnd', 'occupied'].every((key) =>
    integer(value[key]),
  );

type WorkspaceExpectation = Readonly<{
  section: string;
  kind: 'room' | 'stay';
  property?: string;
  status: string;
  roomSummary: boolean;
  requireListMeta: boolean;
}>;

const listMeta = (value: unknown, expectation: WorkspaceExpectation) => {
  if (
    !record(value) ||
    ![
      'section',
      'kind',
      'ids',
      'from',
      'to',
      'page',
      'q',
      'status',
      'building',
      'floor',
      'typeId',
      'hasNext',
    ].every((key) => Object.hasOwn(value, key)) ||
    !Object.keys(value).every((key) =>
      [
        'roomSummary',
        'section',
        'kind',
        'ids',
        'from',
        'to',
        'page',
        'q',
        'status',
        'building',
        'floor',
        'typeId',
        'hasNext',
        'selectedId',
      ].includes(key),
    )
  )
    return undefined;
  if (
    !text(value.section, 40) ||
    value.section !== expectation.section ||
    value.kind !== expectation.kind ||
    !Array.isArray(value.ids) ||
    value.ids.length > 50 ||
    !value.ids.every((id) => identifier(id)) ||
    !text(value.from, 10) ||
    !text(value.to, 10) ||
    !integer(value.page) ||
    value.page > 10_000 ||
    !text(value.q, 200) ||
    !text(value.status, 40) ||
    !text(value.building, 120) ||
    !text(value.floor, 40) ||
    !text(value.typeId, 200) ||
    typeof value.hasNext !== 'boolean' ||
    (expectation.roomSummary ? !roomSummary(value.roomSummary) : value.roomSummary !== undefined) ||
    !(value.selectedId === undefined || identifier(value.selectedId))
  )
    return undefined;
  return value;
};

const bookingReadiness = (value: unknown) =>
  exact(value, ['email', 'accounting', 'onlineAccounting', 'fees', 'connect']) &&
  ['email', 'accounting', 'onlineAccounting', 'fees', 'connect'].every(
    (key) => typeof value[key] === 'boolean',
  );

const snapshot = (value: unknown, expectation: WorkspaceExpectation) => {
  if (
    !record(value) ||
    !['property', 'records'].every((key) => Object.hasOwn(value, key)) ||
    !Object.keys(value).every((key) => ['property', 'records', 'listMeta'].includes(key)) ||
    !property(value.property) ||
    !Array.isArray(value.records) ||
    value.records.length > 4_000 ||
    !value.records.every((row) => record(row) && boardRecord(row)) ||
    (expectation.requireListMeta && !Object.hasOwn(value, 'listMeta')) ||
    !(value.listMeta === undefined || listMeta(value.listMeta, expectation))
  )
    return undefined;
  const ids = new Set<string>();
  for (const row of value.records as Row[]) {
    if (ids.has(String(row.id))) return undefined;
    ids.add(String(row.id));
  }
  const meta = value.listMeta as Row | undefined;
  if (meta) {
    if (!Array.isArray(meta.ids) || meta.ids.some((id) => !ids.has(String(id)))) return undefined;
    if (meta.selectedId !== undefined && !ids.has(String(meta.selectedId))) return undefined;
  }
  return value;
};

function workspace(
  value: unknown,
  actor: NativePmsPageContext,
  expectation: WorkspaceExpectation,
): PmsWorkspace {
  if (
    !record(value) ||
    ![
      'properties',
      'snapshot',
      'defaultDate',
      'readiness',
      'canReviewBookingMedia',
      'bookingReadiness',
      'canManageSetup',
      'permissions',
      'availablePaymentMethods',
      'userId',
      'branches',
      'staff',
      'accounts',
      'charts',
      'parties',
      'products',
      'warehouses',
      'documents',
      'transferRequests',
      'chargeCorrections',
      'stripeConnectionReady',
      'stripePayments',
      'counterRefunds',
      'bookingRefunds',
      'bookingDelivery',
      'ledger',
      'controlBalances',
      'dayControls',
    ].every((key) => Object.hasOwn(value, key)) ||
    !Object.keys(value).every((key) =>
      [
        'properties',
        'snapshot',
        'defaultDate',
        'readiness',
        'frontReady',
        'canReviewBookingMedia',
        'bookingReadiness',
        'canManageSetup',
        'permissions',
        'availablePaymentMethods',
        'userId',
        'branches',
        'staff',
        'accounts',
        'charts',
        'parties',
        'products',
        'warehouses',
        'documents',
        'transferRequests',
        'chargeCorrections',
        'stripeConnectionReady',
        'stripePayments',
        'counterRefunds',
        'bookingRefunds',
        'bookingDelivery',
        'ledger',
        'controlBalances',
        'dayControls',
      ].includes(key),
    )
  )
    return failure('NATIVE_PMS_RESPONSE_INVALID');
  if (
    !Array.isArray(value.properties) ||
    value.properties.length > 100 ||
    !value.properties.every((row) => record(row) && propertySummary(row)) ||
    !Array.isArray(value.readiness) ||
    value.readiness.length > 20 ||
    !value.readiness.every((item) => text(item, 400)) ||
    !bookingReadiness(value.bookingReadiness) ||
    typeof value.canReviewBookingMedia !== 'boolean' ||
    typeof value.canManageSetup !== 'boolean' ||
    !texts(value.permissions) ||
    !value.permissions.includes('pms.read') ||
    !value.permissions.every((permission) => (actor.iam?.permissions ?? []).includes(permission)) ||
    !Array.isArray(value.availablePaymentMethods) ||
    value.availablePaymentMethods.length > 20 ||
    !value.availablePaymentMethods.every((item) => text(item, 40)) ||
    value.userId !== actor.userId ||
    !text(value.defaultDate, 10) ||
    !['branches', 'staff', 'accounts', 'charts', 'parties', 'products', 'warehouses', 'documents'].every(
      (key) => Array.isArray(value[key]) && value[key].length <= 1_000,
    ) ||
    !(value.staff as unknown[]).every((row) => record(row) && exact(row, ['id', 'name']) && identifier(row.id) && text(row.name, 400)) ||
    ![
      'transferRequests',
      'chargeCorrections',
      'stripePayments',
      'counterRefunds',
      'bookingRefunds',
      'bookingDelivery',
    ].every((key) => Array.isArray(value[key]) && value[key].length === 0) ||
    typeof value.stripeConnectionReady !== 'boolean' ||
    !(value.ledger === null || record(value.ledger)) ||
    !(value.controlBalances === null || record(value.controlBalances)) ||
    !(value.dayControls === null || record(value.dayControls)) ||
    !(value.frontReady === undefined || typeof value.frontReady === 'boolean') ||
    !(value.snapshot === null || snapshot(value.snapshot, expectation))
  )
    return failure('NATIVE_PMS_RESPONSE_INVALID');
  if (value.snapshot) {
    const snap = value.snapshot as Row;
    const selected = snap.property as Row;
    if (
      selected.companyId !== actor.companyId ||
      (expectation.property !== undefined && selected.id !== expectation.property) ||
      !value.properties.some((row) => (row as Row).id === selected.id)
    )
      return failure('NATIVE_PMS_RESPONSE_INVALID');
    const meta = snap.listMeta as Row | undefined;
    if (
      meta &&
      (meta.section !== expectation.section ||
        meta.kind !== expectation.kind ||
        !value.permissions.includes('pms.read') ||
        (meta.status !== '' && meta.status !== expectation.status))
    )
      return failure('NATIVE_PMS_RESPONSE_INVALID');
  }
  return value as unknown as PmsWorkspace;
}

function context(
  value: unknown,
  actor: NativePmsPageContext,
  input: Pick<NativePmsReadInput, 'branchSlug' | 'departmentSlug'>,
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
    return failure('NATIVE_PMS_RESPONSE_INVALID');
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
    !texts(parsed.modules) ||
    !sameSet(actor.modules, parsed.modules) ||
    !texts(parsed.permissions) ||
    !sameSet(actor.iam?.permissions ?? [], parsed.permissions)
  )
    return failure('NATIVE_PMS_CONTEXT_CHANGED');
  return parsed;
}

const isoDate = (value: unknown): value is string =>
  text(value, 10) &&
  /^\d{4}-\d{2}-\d{2}$/.test(value) &&
  !Number.isNaN(Date.parse(`${value}T00:00:00Z`));

/** RFC 3339 with an explicit offset, matching the owner's observation input. */
const observation = (value: unknown): value is string =>
  text(value, 64) &&
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(value) &&
  !Number.isNaN(Date.parse(value));

/** Counts and sums are safe integers; net pickup differences may be negative. */
const signedInteger = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value);

const reportDay = (value: unknown): value is NativePmsReportDay =>
  exact(
    value,
    [
      'date',
      'capacity',
      'sold',
      'roomRevenue',
      'netRevenue',
      'tax',
      'refunds',
      'cancellations',
      'noShows',
    ],
  ) &&
  isoDate(value.date) &&
  [
    'capacity',
    'sold',
    'roomRevenue',
    'netRevenue',
    'tax',
    'refunds',
    'cancellations',
    'noShows',
  ].every((key) => integer(value[key]));

const reportTotals = (value: unknown): value is NativePmsReportTotals =>
  exact(
    value,
    [
      'capacity',
      'sold',
      'roomRevenue',
      'netRevenue',
      'tax',
      'refunds',
      'cancellations',
      'noShows',
      'occupancy',
      'adr',
      'revpar',
    ],
  ) &&
  [
    'capacity',
    'sold',
    'roomRevenue',
    'netRevenue',
    'tax',
    'refunds',
    'cancellations',
    'noShows',
    'adr',
    'revpar',
  ].every((key) => integer(value[key])) &&
  typeof value.occupancy === 'number' &&
  Number.isFinite(value.occupancy) &&
  value.occupancy >= 0;

const pacePoint = (value: unknown): value is NativePmsPacePoint =>
  exact(value, ['asOf', 'coverageSince', 'covered', 'reservations', 'roomNights', 'quotedTotal']) &&
  observation(value.asOf) &&
  (value.coverageSince === null || observation(value.coverageSince)) &&
  typeof value.covered === 'boolean' &&
  integer(value.reservations) &&
  integer(value.roomNights) &&
  integer(value.quotedTotal);

const netPickup = (value: unknown): value is NativePmsNetPickup =>
  exact(value, ['reservations', 'roomNights', 'quotedTotal']) &&
  signedInteger(value.reservations) &&
  signedInteger(value.roomNights) &&
  signedInteger(value.quotedTotal);

const pace = (value: unknown): value is NativePmsReportPace =>
  exact(value, ['current', 'previous', 'netPickup']) &&
  (value.current === null || pacePoint(value.current)) &&
  (value.previous === null || pacePoint(value.previous)) &&
  (value.netPickup === null || netPickup(value.netPickup));

const channel = (value: unknown): value is NativePmsChannelReport =>
  exact(value, [
    'source',
    'reservations',
    'roomNights',
    'quotedTotal',
    'cancelled',
    'noShows',
    'recentPickup',
    'unknownCreation',
  ]) &&
  text(value.source, 200) &&
  integer(value.reservations) &&
  integer(value.roomNights) &&
  integer(value.quotedTotal) &&
  integer(value.cancelled) &&
  integer(value.noShows) &&
  integer(value.recentPickup) &&
  integer(value.unknownCreation);

const financeBalances = (value: unknown): value is NativePmsFinanceBalances =>
  exact(value, ['receivable', 'advance', 'deposit', 'refundPayable', 'unallocated']) &&
  ['receivable', 'advance', 'deposit', 'refundPayable', 'unallocated'].every((key) =>
    signedInteger((value as Row)[key]),
  );

const reportData = (
  value: unknown,
  input: Extract<NativePmsReportSelection, { view: 'range' }>,
): NativePmsReportData => {
  if (
    !exact(
      value,
      [
        'property',
        'businessDate',
        'currency',
        'from',
        'to',
        'days',
        'totals',
        'flow',
        'balances',
        'nightAudit',
        'asOf',
        'pace',
        'channels',
        'financeBalances',
      ],
    ) ||
    !exact(value.property, ['id', 'name', 'branchId', 'state', 'businessDate']) ||
    !identifier(value.property.id) ||
    !text(value.property.name, 400) ||
    !identifier(value.property.branchId) ||
    !text(value.property.state, 40) ||
    !isoDate(value.property.businessDate) ||
    !isoDate(value.businessDate) ||
    !text(value.currency, 40) ||
    value.from !== input.from ||
    value.to !== input.to ||
    !Array.isArray(value.days) ||
    value.days.length > 93 ||
    !value.days.every(reportDay) ||
    !reportTotals(value.totals) ||
    !exact(value.flow, ['arrivals', 'departures', 'inHouse']) ||
    !['arrivals', 'departures', 'inHouse'].every((key) => integer((value.flow as Row)[key])) ||
    !exact(value.balances, [
      'source',
      'due',
      'advance',
      'deposit',
      'totalAccounts',
      'openAccounts',
      'creditAccounts',
      'accountsWithDue',
    ]) ||
    !text(value.balances.source, 200) ||
    !['due', 'advance', 'deposit', 'totalAccounts', 'openAccounts', 'creditAccounts', 'accountsWithDue'].every(
      (key) => integer((value.balances as Row)[key]),
    ) ||
    !exact(value.nightAudit, ['businessDate', 'nightState', 'jobState', 'ready', 'blockers']) ||
    !isoDate(value.nightAudit.businessDate) ||
    !(value.nightAudit.nightState === null || text(value.nightAudit.nightState, 40)) ||
    !(value.nightAudit.jobState === null || text(value.nightAudit.jobState, 40)) ||
    typeof value.nightAudit.ready !== 'boolean' ||
    !texts(value.nightAudit.blockers) ||
    !observation(value.asOf) ||
    !pace(value.pace) ||
    !Array.isArray(value.channels) ||
    value.channels.length > 100 ||
    !value.channels.every(channel) ||
    !financeBalances(value.financeBalances)
  )
    return failure('NATIVE_PMS_RESPONSE_INVALID');
  return value as unknown as NativePmsReportData;
};

const reportEntriesData = (
  value: unknown,
  input: Extract<NativePmsReportSelection, { view: 'entries' }>,
): NativePmsReportEntriesData => {
  if (
    !exact(value, ['date', 'page', 'rows', 'hasNext']) ||
    value.date !== input.date ||
    value.page !== input.page ||
    !Array.isArray(value.rows) ||
    value.rows.length > 50 ||
    !value.rows.every(
      (row) => exact(row, ['id', 'data']) && identifier(row.id) && record(row.data),
    ) ||
    typeof value.hasNext !== 'boolean'
  )
    return failure('NATIVE_PMS_RESPONSE_INVALID');
  return value as unknown as NativePmsReportEntriesData;
};

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativePmsConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_PMS_CONFIGURATION_INVALID');
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
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_PMS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_PMS_CONFIGURATION_INVALID');
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
      return failure('NATIVE_PMS_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

type NativePmsAuthorityInput = Readonly<{
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

const frontDeskStatuses = new Set<string>([
  '',
  'arrivals',
  'departures',
  'reserved',
  'in_house',
  'checked_out',
  'cancelled',
  'no_show',
  'completed',
]);

function authority(
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativePmsAuthorityInput,
  invalidCode = 'NATIVE_PMS_REQUEST_INVALID',
) {
  const forwarded = incoming
    .get('x-forwarded-host')
    ?.split(',')[0]
    ?.trim();
  const host = forwarded && forwarded.length > 0 ? forwarded : incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug)
    throw new NativePmsError('NATIVE_PMS_AUTHORITY_INVALID');
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      throw new NativePmsError('NATIVE_PMS_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        throw new NativePmsError(invalidCode);
      headers.set(key, slug);
    }
  }
  if (input.expected) {
    if (
      !identifier(input.expected.companyId) ||
      !identifier(input.expected.assignmentId) ||
      !integer(input.expected.revision) ||
      !integer(input.expected.assignmentVersion, 1)
    )
      throw new NativePmsError(invalidCode);
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  return headers;
}

function dispatch(
  transport: NativePmsTransport,
  url: URL,
  headers: Headers,
  signal?: AbortSignal,
  method: 'GET' | 'POST' = 'GET',
  body?: string,
): Promise<Response> {
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, 30000);
  return transport(method, url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

function transportCall(
  transport: NativePmsTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativePmsReadInput,
  signal?: AbortSignal,
): Promise<Response> {
  const headers = authority(holdings, incoming, input);
  if (!['rooms', 'housekeeping'].includes(input.section))
    throw new NativePmsError('NATIVE_PMS_REQUEST_INVALID');
  if (input.page !== undefined && !integer(input.page))
    throw new NativePmsError('NATIVE_PMS_REQUEST_INVALID');
  if (input.property !== undefined && !identifier(input.property))
    throw new NativePmsError('NATIVE_PMS_REQUEST_INVALID');
  if (input.selectedId !== undefined && !identifier(input.selectedId))
    throw new NativePmsError('NATIVE_PMS_REQUEST_INVALID');
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/pms/rooms`, base);
  const query: Array<[string, string | undefined]> = [
    ['section', input.section],
    ['property', input.property],
    ['status', input.status],
    ['q', input.q],
    ['building', input.building],
    ['floor', input.floor],
    ['typeId', input.typeId],
    ['page', input.page === undefined ? undefined : String(input.page)],
    ['selectedId', input.selectedId],
    ['from', input.from],
    ['to', input.to],
  ];
  for (const [key, value] of query)
    if (value !== undefined && value !== '') url.searchParams.set(key, value);
  return dispatch(transport, url, headers, signal);
}

function frontDeskTransportCall(
  transport: NativePmsTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativePmsFrontDeskInput,
  signal?: AbortSignal,
): Promise<Response> {
  const headers = authority(
    holdings,
    incoming,
    input,
    'NATIVE_PMS_FRONT_DESK_REQUEST_INVALID',
  );
  if (input.status !== undefined && !frontDeskStatuses.has(input.status))
    throw new NativePmsError('NATIVE_PMS_FRONT_DESK_REQUEST_INVALID');
  if (input.page !== undefined && !integer(input.page))
    throw new NativePmsError('NATIVE_PMS_FRONT_DESK_REQUEST_INVALID');
  if (input.property !== undefined && !identifier(input.property))
    throw new NativePmsError('NATIVE_PMS_FRONT_DESK_REQUEST_INVALID');
  if (input.selectedId !== undefined && !identifier(input.selectedId))
    throw new NativePmsError('NATIVE_PMS_FRONT_DESK_REQUEST_INVALID');
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/pms/front-desk`,
    base,
  );
  const query: Array<[string, string | undefined]> = [
    ['property', input.property],
    ['status', input.status],
    ['q', input.q],
    ['page', input.page === undefined ? undefined : String(input.page)],
    ['selectedId', input.selectedId],
    ['from', input.from],
    ['to', input.to],
  ];
  for (const [key, value] of query)
    if (value !== undefined && value !== '') url.searchParams.set(key, value);
  return dispatch(transport, url, headers, signal);
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
        return failure('NATIVE_PMS_RESPONSE_LIMIT');
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
    return failure('NATIVE_PMS_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_PMS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PMS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 4 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_PMS_RESPONSE_INVALID');
  }
}

const directHttp: NativePmsTransport = (method, url, headers, body, signal) =>
  new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const req = request(
      url,
      {
        method,
        agent,
        headers: Object.fromEntries(headers),
        signal,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        const chunks: Uint8Array[] = [];
        let total = 0;
        incoming.on('data', (chunk: Uint8Array) => {
          total += chunk.byteLength;
          if (total > 4 * 1024 * 1024) {
            incoming.destroy();
            reject(new Error('NATIVE_PMS_RESPONSE_LIMIT'));
            return;
          }
          chunks.push(chunk);
        });
        incoming.on('end', () => {
          const bytes = new Uint8Array(total);
          let offset = 0;
          for (const chunk of chunks) {
            bytes.set(chunk, offset);
            offset += chunk.byteLength;
          }
          resolve(
            new Response(bytes, {
              status: incoming.statusCode ?? 0,
              headers: new Headers(
                Object.entries(incoming.headers).flatMap<[string, string]>(([key, value]) =>
                  value === undefined
                    ? []
                    : [[key, Array.isArray(value) ? value.join(', ') : value]],
                ),
              ),
            }),
          );
        });
      },
    );
    req.on('error', reject);
    req.end(body);
  });

export function createNativePmsClient(
  raw: NativePmsConfiguration,
  transport: NativePmsTransport = directHttp,
) {
  const config = configuration(raw);
  return {
    roomBoard: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativePmsPageContext,
      input: NativePmsReadInput,
      signal?: AbortSignal,
    ) => {
      if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
        return Promise.reject(new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED'));
      try {
        const response = await transportCall(
          transport,
          config.base,
          config.holdings,
          incoming,
          input,
          signal,
        );
        const value = await boundedJson(response);
        if (!exact(value, ['data', 'context'])) return failure('NATIVE_PMS_RESPONSE_INVALID');
        context(value.context, actor, input);
        return workspace(value.data, actor, {
          section: input.section,
          kind: 'room',
          ...(input.property !== undefined ? { property: input.property } : {}),
          status: input.status ?? '',
          roomSummary: true,
          requireListMeta: false,
        });
      } catch (error) {
        if (error instanceof NativePmsError) throw error;
        return failure('NATIVE_PMS_UNAVAILABLE');
      }
    },
    frontDesk: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativePmsPageContext,
      input: NativePmsFrontDeskInput,
      signal?: AbortSignal,
    ) => {
      if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
        return Promise.reject(new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED'));
      try {
        const response = await frontDeskTransportCall(
          transport,
          config.base,
          config.holdings,
          incoming,
          input,
          signal,
        );
        const value = await boundedJson(response);
        if (!exact(value, ['data', 'context'])) return failure('NATIVE_PMS_RESPONSE_INVALID');
        context(value.context, actor, input);
        return workspace(value.data, actor, {
          section: 'front-desk',
          kind: 'stay',
          ...(input.property !== undefined ? { property: input.property } : {}),
          status: input.status ?? '',
          roomSummary: false,
          requireListMeta: true,
        });
      } catch (error) {
        if (error instanceof NativePmsError) throw error;
        return failure('NATIVE_PMS_UNAVAILABLE');
      }
    },
    report: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativePmsPageContext,
      input: NativePmsReportInput,
      signal?: AbortSignal,
    ) => {
      if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
        return Promise.reject(new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED'));
      try {
        const headers = authority(config.holdings, incoming, input, 'NATIVE_PMS_REPORT_REQUEST_INVALID');
        const origin = incoming.get('origin');
        if (origin) headers.set('origin', origin);
        if (!identifier(input.property))
          throw new NativePmsError('NATIVE_PMS_REPORT_REQUEST_INVALID');
        const url = new URL(
          `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/pms/reports/${input.view}`,
          config.base,
        );
        url.searchParams.set('property', input.property);
        if (input.view === 'range') {
          if (!isoDate(input.from) || !isoDate(input.to))
            throw new NativePmsError('NATIVE_PMS_REPORT_REQUEST_INVALID');
          url.searchParams.set('from', input.from);
          url.searchParams.set('to', input.to);
          for (const key of ['asOf', 'compareAsOf'] as const) {
            const value = input[key];
            if (value !== undefined) {
              if (!observation(value))
                throw new NativePmsError('NATIVE_PMS_REPORT_REQUEST_INVALID');
              url.searchParams.set(key, value);
            }
          }
        } else {
          if (!isoDate(input.date) || !integer(input.page, 1) || input.page > 10_000)
            throw new NativePmsError('NATIVE_PMS_REPORT_REQUEST_INVALID');
          url.searchParams.set('date', input.date);
          url.searchParams.set('page', String(input.page));
        }
        const response = await dispatch(transport, url, headers, signal);
        const value = await boundedJson(response);
        if (!exact(value, ['data', 'context'])) return failure('NATIVE_PMS_RESPONSE_INVALID');
        context(value.context, actor, input);
        return input.view === 'range'
          ? reportData(value.data, input)
          : reportEntriesData(value.data, input);
      } catch (error) {
        if (error instanceof NativePmsError) throw error;
        return failure('NATIVE_PMS_UNAVAILABLE');
      }
    },
    importCommand: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativePmsPageContext,
      input: NativePmsImportCommandInput,
      signal?: AbortSignal,
    ) => {
      if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
        return Promise.reject(new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED'));
      try {
        const headers = authority(config.holdings, incoming, input);
        const origin = incoming.get('origin');
        if (!origin) throw new NativePmsError('NATIVE_PMS_AUTHORITY_INVALID');
        headers.set('origin', origin);
        headers.set('content-type', 'application/json');
        headers.set('idempotency-key', input.key);
        headers.set('x-erpx-actor-user', input.userId);
        if (!['preview', 'approve', 'balance', 'document', 'property', 'applied'].includes(input.command))
          throw new NativePmsError('NATIVE_PMS_REQUEST_INVALID');
        const url = new URL(
          `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/pms/import-command`,
          config.base,
        );
        if (input.branchSlug) url.searchParams.set('branch', input.branchSlug);
        if (input.departmentSlug) url.searchParams.set('department', input.departmentSlug);
        const response = await dispatch(
          transport,
          url,
          headers,
          signal,
          'POST',
          JSON.stringify({ command: input.command, facts: input.facts }),
        );
        const value = await boundedJson(response);
        if (!exact(value, ['data', 'context'])) return failure('NATIVE_PMS_RESPONSE_INVALID');
        context(value.context, actor, input);
        return value.data;
      } catch (error) {
        if (error instanceof NativePmsError) throw error;
        return failure('NATIVE_PMS_UNAVAILABLE');
      }
    },
    guestAccountCommand: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativePmsPageContext,
      input: NativePmsGuestAccountCommandInput,
      signal?: AbortSignal,
    ) => {
      if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
        return Promise.reject(new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED'));
      try {
        const headers = authority(config.holdings, incoming, input);
        const origin = incoming.get('origin');
        if (!origin) throw new NativePmsError('NATIVE_PMS_AUTHORITY_INVALID');
        headers.set('origin', origin);
        headers.set('content-type', 'application/json');
        headers.set('idempotency-key', input.key);
        const url = new URL(
          `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/pms/guest-account`,
          config.base,
        );
        if (input.branchSlug) url.searchParams.set('branch', input.branchSlug);
        if (input.departmentSlug) url.searchParams.set('department', input.departmentSlug);
        const response = await dispatch(
          transport,
          url,
          headers,
          signal,
          'POST',
          JSON.stringify(input.body),
        );
        const value = await boundedJson(response);
        if (!exact(value, ['data', 'context'])) return failure('NATIVE_PMS_RESPONSE_INVALID');
        context(value.context, actor, input);
        return value.data;
      } catch (error) {
        if (error instanceof NativePmsError) throw error;
        return failure('NATIVE_PMS_UNAVAILABLE');
      }
    },
  };
}

export function nativePmsConfiguration(raw: string): NativePmsConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return failure('NATIVE_PMS_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !exact(value, ['privateBaseUrl', 'timeoutMs', 'holdings']) ||
    !text(value.privateBaseUrl, 200) ||
    !integer(value.timeoutMs, 1) ||
    !Array.isArray(value.holdings) ||
    value.holdings.length > 64 ||
    !value.holdings.every(
      (holding) =>
        exact(holding, ['origin', 'holdingSlug', 'holdingId']) &&
        text(holding.origin, 200) &&
        identifier(holding.holdingId) &&
        text(holding.holdingSlug, 128),
    )
  )
    return failure('NATIVE_PMS_CONFIGURATION_INVALID');
  return value as unknown as NativePmsConfiguration;
}

export function nativePmsFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePmsError)) return undefined;
  if (error.code === 'NATIVE_PMS_CONTEXT_CHANGED' || error.code === 'NATIVE_PMS_AUTHORITY_INVALID')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before submitting.',
    };
  if (
    error.code === 'NATIVE_PMS_CONFIGURATION_INVALID' ||
    error.code === 'NATIVE_PMS_UNAVAILABLE' ||
    error.code === 'NATIVE_PMS_RESPONSE_INVALID' ||
    error.code === 'NATIVE_PMS_RESPONSE_LIMIT' ||
    error.code === 'NATIVE_PMS_COOKIE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_PMS_UNAVAILABLE',
      message: 'The native Hotel board is unavailable.',
    };
  if (error.code === 'NATIVE_PMS_FRONT_DESK_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PMS_FRONT_DESK_REQUEST_INVALID',
      message: 'Use one bounded front-desk selection.',
    };
  if (error.code === 'NATIVE_PMS_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PMS_BOARD_REQUEST_INVALID',
      message: 'Use one bounded room board selection.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_PMS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: error.code, message: 'Hotel item not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_PMS_UNAVAILABLE',
      message: 'The native Hotel board is unavailable.',
    };
  return undefined;
}
