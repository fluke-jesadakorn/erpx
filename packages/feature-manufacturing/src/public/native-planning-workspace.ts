import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type {
  NativePlanningAllocation,
  NativePlanningCalendarDay,
  NativePlanningContext,
  NativePlanningRecommendation,
  NativePlanningResource,
  NativePlanningScenario,
  NativePlanningShortage,
  NativePlanningWorkspace,
} from '../lib/native-planning-types';

/** Rendering input from retained page authorization, never a grant. */
export type NativePlanningPageContext = {
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

export type NativePlanningConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativePlanningWorkspaceReadInput = Readonly<{
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

export type NativePlanningTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativePlanningError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Planning request could not be completed.');
    this.name = 'NativePlanningError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativePlanningError(code, status, message);
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
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const trimmed = (value: unknown, minimum: number, maximum: number): value is string =>
  text(value, maximum) &&
  [...value].length >= minimum &&
  value.trim() === value &&
  !/[\u0000-\u001f\u007f]/.test(value);
const date = (value: unknown): value is string =>
  text(value, 10) && /^\d{4}-\d{2}-\d{2}$/.test(value);
const optionalIdentifier = (value: unknown) => value === null || identifier(value);
const statuses = new Set(['calculated', 'released', 'cancelled']);
const recommendationStatuses = new Set(['proposed', 'approved', 'rejected']);
const resourceStatuses = new Set(['active', 'archived']);
const MAX_SCENARIOS = 200;
const MAX_RECOMMENDATIONS = 200;
const MAX_RESOURCES = 200;
const MAX_CALENDAR = 500;
const MAX_ALLOCATIONS = 5_000;
const MAX_SHORTAGES = 1_000;
const MAX_CONTEXT_ROWS = 1_000;

const shortageRow = (value: unknown): value is NativePlanningShortage =>
  exact(value, [
    'productId',
    'requiredMilli',
    'availableMilli',
    'incomingMilli',
    'shortageMilli',
  ]) &&
  identifier(value.productId) &&
  integer(value.requiredMilli) &&
  integer(value.availableMilli) &&
  integer(value.incomingMilli) &&
  integer(value.shortageMilli) &&
  value.shortageMilli === Math.max(0, value.requiredMilli - value.availableMilli - value.incomingMilli);

function contextRow(value: unknown): value is NativePlanningContext {
  if (!record(value)) return false;
  const allowed = new Set([
    'demandSource',
    'incomingSupply',
    'stock',
    'allocations',
    'operations',
    'startDate',
    'capacityBasis',
    'comparisonScenarioId',
    'comparison',
  ]);
  if (Object.keys(value).some((key) => !allowed.has(key))) return false;
  if (
    !Array.isArray(value.incomingSupply) ||
    value.incomingSupply.length > MAX_CONTEXT_ROWS ||
    !Array.isArray(value.stock) ||
    value.stock.length > MAX_CONTEXT_ROWS ||
    !Array.isArray(value.allocations) ||
    value.allocations.length > MAX_CONTEXT_ROWS ||
    !Array.isArray(value.operations) ||
    value.operations.length > 50
  )
    return false;
  if (
    value.demandSource !== undefined &&
    !(
      exact(value.demandSource, ['id', 'version', 'quantityMilli']) &&
      identifier(value.demandSource.id) &&
      integer(value.demandSource.version, 1) &&
      integer(value.demandSource.quantityMilli, 1)
    )
  )
    return false;
  if (
    !value.incomingSupply.every(
      (row) =>
        exact(row, ['id', 'version', 'productId', 'quantityMilli', 'dueDate']) &&
        identifier(row.id) &&
        integer(row.version, 1) &&
        identifier(row.productId) &&
        integer(row.quantityMilli) &&
        date(row.dueDate),
    ) ||
    !value.stock.every(
      (row) =>
        exact(row, ['productId', 'version']) &&
        identifier(row.productId) &&
        integer(row.version),
    ) ||
    !value.allocations.every(
      (row) =>
        exact(row, [
          'resourceId',
          'workDate',
          'minutes',
          'resourceVersion',
          'calendarVersion',
          'availableMinutes',
        ]) &&
        identifier(row.resourceId) &&
        date(row.workDate) &&
        integer(row.minutes, 1) &&
        integer(row.resourceVersion, 1) &&
        integer(row.calendarVersion) &&
        integer(row.availableMinutes) &&
        row.availableMinutes <= 1_440,
    ) ||
    !value.operations.every(
      (row) =>
        exact(row, ['id', 'name', 'resourceId', 'requiredMinutes']) &&
        identifier(row.id) &&
        trimmed(row.name, 1, 160) &&
        identifier(row.resourceId) &&
        integer(row.requiredMinutes),
    )
  )
    return false;
  if (value.startDate !== undefined && !date(value.startDate)) return false;
  if (value.capacityBasis !== undefined && !trimmed(value.capacityBasis, 1, 80)) return false;
  if (value.comparisonScenarioId !== undefined && !identifier(value.comparisonScenarioId))
    return false;
  if (
    value.comparison !== undefined &&
    !(
      exact(value.comparison, [
        'scenarioId',
        'sourceVersion',
        'shortageDeltaMilli',
        'requiredMinutesDelta',
        'feasibleDate',
        'currentFeasibleDate',
      ]) &&
      identifier(value.comparison.scenarioId) &&
      integer(value.comparison.sourceVersion, 1) &&
      typeof value.comparison.shortageDeltaMilli === 'number' &&
      Number.isSafeInteger(value.comparison.shortageDeltaMilli) &&
      typeof value.comparison.requiredMinutesDelta === 'number' &&
      Number.isSafeInteger(value.comparison.requiredMinutesDelta) &&
      date(value.comparison.feasibleDate) &&
      date(value.comparison.currentFeasibleDate)
    )
  )
    return false;
  return true;
}

const scenarioRow = (value: unknown): value is NativePlanningScenario =>
  exact(value, [
    'id', 'company_id', 'branch_id', 'warehouse_id', 'specification_id',
    'specification_version', 'demand_quantity_milli', 'due_date',
    'capacity_minutes', 'status', 'created_by', 'version', 'created_at',
    'updated_at', 'shortages', 'context',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  identifier(value.branch_id) &&
  identifier(value.warehouse_id) &&
  identifier(value.specification_id) &&
  integer(value.specification_version, 1) &&
  integer(value.demand_quantity_milli, 1) &&
  date(value.due_date) &&
  integer(value.capacity_minutes) &&
  typeof value.status === 'string' &&
  statuses.has(value.status) &&
  identifier(value.created_by) &&
  integer(value.version, 1) &&
  integer(value.created_at) &&
  integer(value.updated_at) &&
  value.updated_at >= value.created_at &&
  Array.isArray(value.shortages) &&
  value.shortages.length <= MAX_SHORTAGES &&
  value.shortages.every(shortageRow) &&
  contextRow(value.context);

const recommendationRow = (value: unknown): value is NativePlanningRecommendation =>
  exact(value, [
    'id', 'company_id', 'branch_id', 'scenario_id', 'scenario_version',
    'recommended_quantity_milli', 'required_minutes', 'feasible_date',
    'status', 'created_by', 'reviewed_by', 'version', 'created_at',
    'updated_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  identifier(value.branch_id) &&
  identifier(value.scenario_id) &&
  integer(value.scenario_version, 1) &&
  integer(value.recommended_quantity_milli, 1) &&
  integer(value.required_minutes) &&
  date(value.feasible_date) &&
  typeof value.status === 'string' &&
  recommendationStatuses.has(value.status) &&
  identifier(value.created_by) &&
  optionalIdentifier(value.reviewed_by) &&
  (value.status === 'proposed'
    ? value.reviewed_by === null
    : value.status === 'approved'
      ? identifier(value.reviewed_by) && value.reviewed_by !== value.created_by
      : identifier(value.reviewed_by)) &&
  integer(value.version, 1) &&
  integer(value.created_at) &&
  integer(value.updated_at) &&
  value.updated_at >= value.created_at;

const resourceRow = (value: unknown): value is NativePlanningResource =>
  exact(value, [
    'id', 'company_id', 'branch_id', 'code', 'name', 'daily_minutes',
    'work_weekdays', 'status', 'version', 'created_by', 'created_at',
    'updated_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  identifier(value.branch_id) &&
  trimmed(value.code, 1, 64) &&
  trimmed(value.name, 1, 160) &&
  integer(value.daily_minutes) &&
  value.daily_minutes <= 1_440 &&
  Array.isArray(value.work_weekdays) &&
  value.work_weekdays.length >= 1 &&
  value.work_weekdays.length <= 7 &&
  value.work_weekdays.every((day) => integer(day) && day <= 6) &&
  new Set(value.work_weekdays).size === value.work_weekdays.length &&
  typeof value.status === 'string' &&
  resourceStatuses.has(value.status) &&
  integer(value.version, 1) &&
  identifier(value.created_by) &&
  integer(value.created_at) &&
  integer(value.updated_at) &&
  value.updated_at >= value.created_at;

const calendarRow = (value: unknown): value is NativePlanningCalendarDay =>
  exact(value, [
    'resource_id', 'work_date', 'available_minutes', 'reason', 'version',
    'updated_by', 'updated_at',
  ]) &&
  identifier(value.resource_id) &&
  date(value.work_date) &&
  integer(value.available_minutes) &&
  value.available_minutes <= 1_440 &&
  trimmed(value.reason, 1, 3_000) &&
  integer(value.version, 1) &&
  identifier(value.updated_by) &&
  integer(value.updated_at);

const allocationRow = (value: unknown): value is NativePlanningAllocation =>
  exact(value, [
    'id', 'scenario_id', 'resource_id', 'work_date', 'minutes', 'created_by',
    'created_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.scenario_id) &&
  identifier(value.resource_id) &&
  date(value.work_date) &&
  integer(value.minutes, 1) &&
  identifier(value.created_by) &&
  integer(value.created_at);

function workspaceData(
  value: unknown,
  actor: NativePlanningPageContext,
  input: NativePlanningWorkspaceReadInput,
): NativePlanningWorkspace {
  if (
    !exact(value, ['scenarios', 'recommendations', 'resources', 'calendar', 'allocations']) ||
    !Array.isArray(value.scenarios) ||
    value.scenarios.length > MAX_SCENARIOS ||
    !value.scenarios.every(scenarioRow) ||
    !Array.isArray(value.recommendations) ||
    value.recommendations.length > MAX_RECOMMENDATIONS ||
    !value.recommendations.every(recommendationRow) ||
    !Array.isArray(value.resources) ||
    value.resources.length > MAX_RESOURCES ||
    !value.resources.every(resourceRow) ||
    !Array.isArray(value.calendar) ||
    value.calendar.length > MAX_CALENDAR ||
    !value.calendar.every(calendarRow) ||
    !Array.isArray(value.allocations) ||
    value.allocations.length > MAX_ALLOCATIONS ||
    !value.allocations.every(allocationRow)
  )
    return failure('NATIVE_PLANNING_RESPONSE_INVALID');

  const scenarios = value.scenarios as NativePlanningScenario[];
  const recommendations = value.recommendations as NativePlanningRecommendation[];
  const resources = value.resources as NativePlanningResource[];
  const calendar = value.calendar as NativePlanningCalendarDay[];
  const allocations = value.allocations as NativePlanningAllocation[];
  const scenarioIds = new Set<string>();
  const recommendationIds = new Set<string>();
  const recommendedScenarios = new Set<string>();
  const resourceIds = new Set<string>();
  const calendarKeys = new Set<string>();
  const allocationIds = new Set<string>();

  for (const scenario of scenarios) {
    if (
      scenario.company_id !== actor.companyId ||
      (actor.branchId !== null && scenario.branch_id !== actor.branchId) ||
      scenarioIds.has(scenario.id) ||
      new Set(scenario.shortages.map((shortage) => shortage.productId)).size !==
        scenario.shortages.length
    )
      return failure('NATIVE_PLANNING_RESPONSE_INVALID');
    scenarioIds.add(scenario.id);
  }
  for (const recommendation of recommendations) {
    if (
      recommendation.company_id !== actor.companyId ||
      (actor.branchId !== null && recommendation.branch_id !== actor.branchId) ||
      recommendationIds.has(recommendation.id) ||
      recommendedScenarios.has(recommendation.scenario_id) ||
      !scenarioIds.has(recommendation.scenario_id)
    )
      return failure('NATIVE_PLANNING_RESPONSE_INVALID');
    recommendationIds.add(recommendation.id);
    recommendedScenarios.add(recommendation.scenario_id);
  }
  for (const resource of resources) {
    if (
      resource.company_id !== actor.companyId ||
      (actor.branchId !== null && resource.branch_id !== actor.branchId) ||
      resourceIds.has(resource.id)
    )
      return failure('NATIVE_PLANNING_RESPONSE_INVALID');
    resourceIds.add(resource.id);
  }
  for (const day of calendar) {
    const key = `${day.resource_id}:${day.work_date}`;
    if (!resourceIds.has(day.resource_id) || calendarKeys.has(key))
      return failure('NATIVE_PLANNING_RESPONSE_INVALID');
    calendarKeys.add(key);
  }
  for (const allocation of allocations) {
    if (
      allocationIds.has(allocation.id) ||
      !scenarioIds.has(allocation.scenario_id) ||
      !resourceIds.has(allocation.resource_id)
    )
      return failure('NATIVE_PLANNING_RESPONSE_INVALID');
    allocationIds.add(allocation.id);
  }
  if (input.expected?.companyId !== undefined && input.expected.companyId !== actor.companyId)
    return failure('NATIVE_PLANNING_CONTEXT_CHANGED');
  return { scenarios, recommendations, resources, calendar, allocations };
}

function context(
  value: unknown,
  actor: NativePlanningPageContext,
  input: NativePlanningWorkspaceReadInput,
): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_PLANNING_RESPONSE_INVALID');
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
    return failure('NATIVE_PLANNING_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_PLANNING_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativePlanningConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_PLANNING_CONFIGURATION_INVALID');
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
    return failure('NATIVE_PLANNING_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_PLANNING_CONFIGURATION_INVALID');
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
      return failure('NATIVE_PLANNING_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativePlanningTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  holdingSlug: string,
  input: NativePlanningWorkspaceReadInput,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    return Promise.reject(new NativePlanningError('NATIVE_PLANNING_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativePlanningError('NATIVE_PLANNING_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativePlanningError('NATIVE_PLANNING_REQUEST_INVALID'));
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
      return Promise.reject(new NativePlanningError('NATIVE_PLANNING_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/planning/workspace`,
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
        return failure('NATIVE_PLANNING_RESPONSE_LIMIT');
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
    return failure('NATIVE_PLANNING_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_PLANNING_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PLANNING_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_PLANNING_RESPONSE_INVALID');
  }
}

export function nativePlanningAvailable(): boolean {
  return process.env.ERPX_NATIVE_PLANNING_CONFIG !== undefined;
}

export function nativePlanningConfiguration(raw: string): NativePlanningConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativePlanningError('NATIVE_PLANNING_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_PLANNING_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_PLANNING_CONFIGURATION_INVALID');
  return value as unknown as NativePlanningConfiguration;
}

export function createNativePlanningClient(
  raw: NativePlanningConfiguration,
  transport: NativePlanningTransport = directHttp,
) {
  const config = configuration(raw);
  const read = (
    incoming: Pick<Headers, 'get'>,
    actor: NativePlanningPageContext,
    input: NativePlanningWorkspaceReadInput,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return Promise.reject(new NativePlanningError('NATIVE_PLANNING_CONTEXT_CHANGED'));
    return transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input.holdingSlug,
      input,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativePlanningError) throw error;
      return failure('NATIVE_PLANNING_UNAVAILABLE');
    });
  };
  return {
    workspace: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativePlanningPageContext,
      input: NativePlanningWorkspaceReadInput,
      signal?: AbortSignal,
    ): Promise<NativePlanningWorkspace> => {
      const value = await boundedJson(await read(incoming, actor, input, signal));
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_PLANNING_RESPONSE_INVALID');
      context(value.context, actor, input);
      return workspaceData(value.data, actor, input);
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
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = request(
      url,
      { method, agent, headers: { ...Object.fromEntries(headers) }, signal, maxHeaderSize: 8192 },
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
