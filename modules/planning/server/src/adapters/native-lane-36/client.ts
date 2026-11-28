import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { Row } from '@erpx/commerce-server/public/commerce';

/**
 * Strict private transport for the owner-local Planning lane-36 scoped reads.
 * The mounted axum owner route is the sole read authority: it owns the SQL,
 * the projection and the Company/branch scope. This bridge emits no SQL,
 * accepts no SQL from a caller and fails closed when the native service is not
 * configured or returns anything other than the exact bounded envelope.
 *
 * Every registered Holding is supported. The target is selected from the
 * caller's WorkContext, or, for legacy repository callers without one, from
 * the exact incoming Host. There is no first-Holding default and an unknown
 * target never reaches the private service.
 */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

export type LaneKind =
  | 'recommendations'
  | 'scenarios'
  | 'resources'
  | 'calendar'
  | 'scenario'
  | 'scoped-recommendation'
  | 'comparison'
  | 'executive-shortage'
  | 'executive-recommendation';
export type ScopedActor = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;
export type LaneIdentity = Readonly<{
  id?: string;
  comparisonScenarioId?: string;
  specificationId?: string;
  from?: string;
  to?: string;
  eligibility?: 'proposed' | 'capacity';
}>;

const ENV = 'ERPX_NATIVE_PLANNING_LANE36_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_ROWS = 500;

const fail = (
  code = 'NATIVE_PLANNING_LANE36_UNAVAILABLE',
  status = 503,
  message = 'The native Planning scoped read is unavailable.',
): never => {
  throw new ApiError(status, code, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const scopeId = (value: unknown): value is string =>
  identifier(value) && /^[A-Za-z0-9_.:-]+$/.test(String(value));
const hostId = (value: unknown): value is string =>
  text(value, 512) && value.length > 0 && !/[\s/?#@]/.test(String(value));
const integer = (value: unknown, min = Number.MIN_SAFE_INTEGER): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const boundedInteger = (value: unknown, min: number, max: number): value is number =>
  integer(value, min) && (value as number) <= max;
const nullableText = (value: unknown, max = 4096): boolean => value === null || text(value, max);
const inSet = (value: unknown, values: readonly string[]): boolean =>
  typeof value === 'string' && values.includes(value);

const KINDS: readonly LaneKind[] = [
  'recommendations', 'scenarios', 'resources', 'calendar', 'scenario', 'scoped-recommendation',
  'comparison', 'executive-shortage', 'executive-recommendation',
];
const isRowKind = (kind: LaneKind): boolean =>
  kind === 'scenario' || kind === 'scoped-recommendation' || kind === 'comparison'
  || kind === 'executive-shortage' || kind === 'executive-recommendation';
const isExecutiveKind = (kind: LaneKind): boolean =>
  kind === 'executive-shortage' || kind === 'executive-recommendation';
const REVIEW_STATUSES = ['proposed', 'approved', 'rejected'];
const SCENARIO_STATUSES = ['calculated', 'released', 'cancelled'];
const RESOURCE_STATUSES = ['active', 'archived'];

const RECOMMENDATION_KEYS = [
  'id', 'scenario_id', 'scenario_version', 'recommended_quantity_milli', 'required_minutes',
  'feasible_date', 'status', 'created_by', 'reviewed_by', 'version', 'created_at', 'updated_at',
  'company_id', 'branch_id',
] as const;
const SCENARIO_LIST_KEYS = [
  'id', 'company_id', 'branch_id', 'warehouse_id', 'specification_id', 'specification_version',
  'demand_quantity_milli', 'due_date', 'capacity_minutes', 'shortages_json', 'status',
  'created_by', 'version', 'created_at', 'updated_at', 'context_json', 'specification_name',
] as const;
const RESOURCE_KEYS = [
  'id', 'company_id', 'branch_id', 'code', 'name', 'daily_minutes', 'work_weekdays_json',
  'status', 'version', 'created_by', 'created_at', 'updated_at',
] as const;
const CALENDAR_KEYS = [
  'resource_id', 'work_date', 'available_minutes', 'reason', 'version', 'updated_by', 'updated_at',
] as const;
const SCENARIO_KEYS = [
  'id', 'company_id', 'branch_id', 'warehouse_id', 'specification_id', 'specification_version',
  'demand_quantity_milli', 'due_date', 'capacity_minutes', 'shortages_json', 'status',
  'created_by', 'version', 'created_at', 'updated_at', 'context_json',
] as const;
const SCOPED_RECOMMENDATION_KEYS = [
  'id', 'scenario_id', 'scenario_version', 'recommended_quantity_milli', 'required_minutes',
  'feasible_date', 'status', 'created_by', 'reviewed_by', 'version', 'created_at', 'updated_at',
  'company_id', 'branch_id', 'warehouse_id', 'specification_id', 'specification_version',
  'context_json', 'scenario_created_by',
] as const;
const COMPARISON_KEYS = [
  'id', 'version', 'shortages_json', 'required_minutes', 'feasible_date',
] as const;
const EXECUTIVE_KEYS = [
  'scenario_id', 'company_id', 'branch_id', 'branch_name', 'warehouse_id', 'warehouse_name',
  'specification_id', 'specification_name', 'output_product_id', 'output_product_name',
  'specification_version', 'demand_quantity_milli', 'due_date', 'capacity_minutes',
  'scenario_status', 'scenario_version', 'scenario_created_at', 'scenario_updated_at',
  'recommendation_id', 'recommendation_status', 'recommended_quantity_milli', 'required_minutes',
  'feasible_date', 'recommendation_version', 'recommendation_created_at',
  'recommendation_updated_at', 'shortage_count', 'shortage_items_json',
] as const;

const recommendationFields = (row: Row): boolean =>
  identifier(row.id) && identifier(row.scenario_id) &&
  integer(row.scenario_version, 1) && integer(row.recommended_quantity_milli, 1) &&
  integer(row.required_minutes, 0) && text(row.feasible_date, 40) &&
  inSet(row.status, REVIEW_STATUSES) && identifier(row.created_by) &&
  nullableText(row.reviewed_by, 200) && integer(row.version, 1) &&
  integer(row.created_at) && integer(row.updated_at) &&
  identifier(row.company_id) && identifier(row.branch_id);
const recommendationShape = (row: unknown): boolean =>
  exact(row, RECOMMENDATION_KEYS) && recommendationFields(row as Row);

function rowValidator(kind: LaneKind): (row: unknown) => boolean {
  if (isExecutiveKind(kind))
    return (row: unknown) =>
      exact(row, EXECUTIVE_KEYS) &&
      identifier(row.scenario_id) && identifier(row.company_id) && identifier(row.branch_id) &&
      nullableText(row.branch_name, 400) && identifier(row.warehouse_id) &&
      nullableText(row.warehouse_name, 400) && identifier(row.specification_id) &&
      nullableText(row.specification_name, 400) && nullableText(row.output_product_id, 200) &&
      nullableText(row.output_product_name, 400) &&
      integer(row.specification_version, 1) && text(row.demand_quantity_milli, 200) &&
      text(row.due_date, 40) && text(row.capacity_minutes, 200) &&
      inSet(row.scenario_status, SCENARIO_STATUSES) && integer(row.scenario_version, 1) &&
      integer(row.scenario_created_at) && integer(row.scenario_updated_at) &&
      nullableText(row.recommendation_id, 200) &&
      (row.recommendation_status === null || inSet(row.recommendation_status, REVIEW_STATUSES)) &&
      nullableText(row.recommended_quantity_milli, 200) && nullableText(row.required_minutes, 200) &&
      nullableText(row.feasible_date, 40) &&
      (row.recommendation_version === null || integer(row.recommendation_version, 1)) &&
      (row.recommendation_created_at === null || integer(row.recommendation_created_at)) &&
      (row.recommendation_updated_at === null || integer(row.recommendation_updated_at)) &&
      text(row.shortage_count, 200) && text(row.shortage_items_json, 1_048_576);
  if (kind === 'recommendations') return recommendationShape;
  if (kind === 'scenarios')
    return (row: unknown) =>
      exact(row, SCENARIO_LIST_KEYS) &&
      identifier(row.id) && identifier(row.company_id) && identifier(row.branch_id) &&
      identifier(row.warehouse_id) && identifier(row.specification_id) &&
      integer(row.specification_version, 1) && integer(row.demand_quantity_milli, 1) &&
      text(row.due_date, 40) && integer(row.capacity_minutes, 0) &&
      text(row.shortages_json, 1_048_576) && inSet(row.status, SCENARIO_STATUSES) &&
      identifier(row.created_by) && integer(row.version, 1) &&
      integer(row.created_at) && integer(row.updated_at) && text(row.context_json, 1_048_576) &&
      text(row.specification_name, 400);
  if (kind === 'resources')
    return (row: unknown) =>
      exact(row, RESOURCE_KEYS) &&
      identifier(row.id) && identifier(row.company_id) && identifier(row.branch_id) &&
      text(row.code, 200) && text(row.name, 400) &&
      boundedInteger(row.daily_minutes, 0, 1440) &&
      text(row.work_weekdays_json, 4096) && inSet(row.status, RESOURCE_STATUSES) &&
      integer(row.version, 1) && identifier(row.created_by) &&
      integer(row.created_at) && integer(row.updated_at);
  if (kind === 'calendar')
    return (row: unknown) =>
      exact(row, CALENDAR_KEYS) &&
      identifier(row.resource_id) && text(row.work_date, 40) &&
      boundedInteger(row.available_minutes, 0, 1440) &&
      text(row.reason, 4096) && integer(row.version, 1) &&
      identifier(row.updated_by) && integer(row.updated_at);
  if (kind === 'scenario')
    return (row: unknown) =>
      exact(row, SCENARIO_KEYS) &&
      identifier(row.id) && identifier(row.company_id) && identifier(row.branch_id) &&
      identifier(row.warehouse_id) && identifier(row.specification_id) &&
      integer(row.specification_version, 1) && integer(row.demand_quantity_milli, 1) &&
      text(row.due_date, 40) && integer(row.capacity_minutes, 0) &&
      text(row.shortages_json, 1_048_576) && inSet(row.status, SCENARIO_STATUSES) &&
      identifier(row.created_by) && integer(row.version, 1) &&
      integer(row.created_at) && integer(row.updated_at) && text(row.context_json, 1_048_576);
  if (kind === 'scoped-recommendation')
    return (row: unknown) =>
      exact(row, SCOPED_RECOMMENDATION_KEYS) &&
      recommendationFields(row as Row) &&
      identifier(row.warehouse_id) && identifier(row.specification_id) &&
      integer(row.specification_version, 1) && text(row.context_json, 1_048_576) &&
      identifier(row.scenario_created_by);
  return (row: unknown) =>
    exact(row, COMPARISON_KEYS) &&
    identifier(row.id) && integer(row.version, 1) &&
    text(row.shortages_json, 1_048_576) && integer(row.required_minutes, 0) &&
    text(row.feasible_date, 40);
}

function settings(raw: string): Config {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return fail('NATIVE_PLANNING_LANE36_CONFIG_INVALID'); }
  if (
    !record(value) || !text(value.privateBaseUrl, 512) ||
    !Number.isSafeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64
  ) return fail('NATIVE_PLANNING_LANE36_CONFIG_INVALID');
  let base: URL;
  try { base = new URL(value.privateBaseUrl as string); } catch { return fail('NATIVE_PLANNING_LANE36_CONFIG_INVALID'); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    RESERVED_PORTS.has(port) || (port >= 3200 && port <= 3299) ||
    base.username || base.password || base.origin !== value.privateBaseUrl
  ) return fail('NATIVE_PLANNING_LANE36_CONFIG_INVALID');
  const holdings: Holding[] = [];
  const hosts = new Set<string>();
  const ids = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) || !text(item.origin, 512) || !hostId(item.host) ||
      !identifier(item.holdingId) ||
      !text(item.holdingSlug, 128) || !/^[a-z0-9_-]+$/.test(String(item.holdingSlug)) ||
      (item.companyId !== undefined && !scopeId(item.companyId))
    ) return fail('NATIVE_PLANNING_LANE36_CONFIG_INVALID');
    let origin: URL;
    try { origin = new URL(String(item.origin)); } catch { return fail('NATIVE_PLANNING_LANE36_CONFIG_INVALID'); }
    if (
      origin.protocol !== 'https:' || origin.origin !== item.origin ||
      origin.username || origin.password || origin.host !== String(item.host) ||
      hosts.has(String(item.host)) || ids.has(String(item.holdingId))
    ) return fail('NATIVE_PLANNING_LANE36_CONFIG_INVALID');
    hosts.add(String(item.host));
    ids.add(String(item.holdingId));
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingSlug: String(item.holdingSlug),
      holdingId: String(item.holdingId),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

/** Selector presence only; an absent configuration never probes the service. */
export function nativePlanningLane36Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

/** Resolve exactly one registered Holding. No default: an actor must name a
 * registered (holdingId, holdingSlug) and a legacy caller must present an
 * exact registered Host. A configured companyId additionally pins the actor. */
async function targetFor(config: Config, actor?: ScopedActor): Promise<Holding> {
  const incoming = await headers().catch(() => null);
  const host = incoming?.get('host') ?? null;
  if (actor) {
    const target = config.holdings.find(
      holding =>
        holding.holdingId === actor.holdingId &&
        holding.holdingSlug === actor.holdingSlug &&
        (holding.companyId === undefined || holding.companyId === actor.companyId),
    );
    if (!target || (host !== null && host !== target.host))
      return fail('WORK_CONTEXT_CHANGED', 409);
    return target;
  }
  const target = host === null ? undefined : config.holdings.find(holding => holding.host === host);
  if (!target) return fail('WORK_CONTEXT_CHANGED', 409);
  return target;
}

async function read(
  kind: LaneKind,
  companyId: string,
  branchId: string | null,
  actor?: ScopedActor,
  identity: LaneIdentity = {},
): Promise<Row[] | Row | null | undefined> {
  const raw = process.env[ENV];
  if (raw === undefined) return undefined;
  if (
    !KINDS.includes(kind) || !scopeId(companyId) ||
    (branchId !== null && !scopeId(branchId)) ||
    Object.values(identity).some(value => value !== undefined && !scopeId(value)) ||
    (isRowKind(kind) && kind !== 'comparison' && identity.id === undefined) ||
    (kind === 'comparison' &&
      (identity.comparisonScenarioId === undefined || identity.specificationId === undefined)) ||
    (isExecutiveKind(kind) &&
      (identity.from === undefined || identity.to === undefined ||
        (kind === 'executive-recommendation' &&
          !inSet(identity.eligibility, ['proposed', 'capacity'])) ||
        (kind === 'executive-shortage' && identity.eligibility !== undefined)))
  ) return fail('PLANNING_LANE36_QUERY_INVALID', 422);
  const config = settings(raw);
  const target = await targetFor(config, actor);
  if (target.companyId !== undefined && target.companyId !== companyId)
    return fail('WORK_CONTEXT_CHANGED', 409);
  const h = new Headers({
    host: target.host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': companyId,
  });
  const incoming = await headers().catch(() => null);
  const cookie = incoming?.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return fail('NATIVE_PLANNING_LANE36_COOKIE_LIMIT');
    h.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/planning/lane-36/${kind}`,
    config.privateBaseUrl,
  );
  if (branchId !== null) url.searchParams.set('branch', branchId);
  if (identity.id !== undefined) url.searchParams.set('id', identity.id);
  if (identity.comparisonScenarioId !== undefined)
    url.searchParams.set('comparisonScenarioId', identity.comparisonScenarioId);
  if (identity.specificationId !== undefined)
    url.searchParams.set('specificationId', identity.specificationId);
  if (identity.from !== undefined) url.searchParams.set('from', identity.from);
  if (identity.to !== undefined) url.searchParams.set('to', identity.to);
  if (identity.eligibility !== undefined) url.searchParams.set('eligibility', identity.eligibility);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET', headers: h, signal: controller.signal, redirect: 'error', cache: 'no-store',
    });
  } catch {
    return fail();
  } finally {
    clearTimeout(timer);
  }
  if (response.status === 403 || response.status === 404) {
    void response.body?.cancel().catch(() => undefined);
    return fail('PLANNING_LANE36_NOT_FOUND', response.status, 'Planning scoped data is not available in this scope.');
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return fail();
  }
  let parsed: unknown;
  try { parsed = await response.json(); } catch { return fail('NATIVE_PLANNING_LANE36_RESPONSE_INVALID', 502); }
  if (!record(parsed)) return fail('NATIVE_PLANNING_LANE36_RESPONSE_INVALID', 502);
  if (record(parsed.error)) {
    const code = parsed.error.code;
    const message = parsed.error.message;
    return fail(
      text(code, 100) ? String(code) : undefined,
      response.status >= 400 && response.status < 500 ? response.status : 503,
      text(message, 300) ? String(message) : undefined,
    );
  }
  if (!response.ok) return fail();
  if (
    parsed.kind !== kind || parsed.companyId !== companyId ||
    parsed.branchId !== (branchId ?? null)
  ) return fail('NATIVE_PLANNING_LANE36_RESPONSE_INVALID', 502);
  const valid = rowValidator(kind);
  const scopedRow = (row: unknown): boolean => {
    if (!valid(row)) return false;
    if (kind === 'calendar' || isRowKind(kind)) return true;
    const scoped = row as Row;
    return scoped.company_id === companyId &&
      (branchId === null || scoped.branch_id === branchId);
  };
  if (isRowKind(kind)) {
    if (parsed.data !== null && !scopedRow(parsed.data))
      return fail('NATIVE_PLANNING_LANE36_RESPONSE_INVALID', 502);
    return parsed.data as Row | null;
  }
  if (!Array.isArray(parsed.data) || parsed.data.length > MAX_ROWS ||
    !parsed.data.every(scopedRow))
    return fail('NATIVE_PLANNING_LANE36_RESPONSE_INVALID', 502);
  return parsed.data as Row[];
}

const scoped = (kind: LaneKind) => async (
  companyId: string,
  branchId: string | null,
  _branchId2: string | null,
  actor?: ScopedActor,
): Promise<{ results: Row[] }> => {
  const rows = await read(kind, companyId, branchId, actor);
  if (rows === undefined) return fail('NATIVE_PLANNING_LANE36_CONFIG_REQUIRED');
  return { results: rows as Row[] };
};

const single = (kind: LaneKind) => async (
  identity: LaneIdentity,
  companyId: string,
  branchId: string | null,
  actor?: ScopedActor,
): Promise<Row | null> => {
  const row = await read(kind, companyId, branchId, actor, identity);
  if (row === undefined) return fail('NATIVE_PLANNING_LANE36_CONFIG_REQUIRED');
  return row as Row | null;
};

/** Scoped recommendation list; sole read authority is the native owner route. */
export const nativeRecommendations = scoped('recommendations');
/** Scenario list; sole read authority is the native owner route. */
export const nativeScenarios = scoped('scenarios');
/** Production resources list; sole read authority is the native owner route. */
export const nativeResources = scoped('resources');
/** Resource calendar list; sole read authority is the native owner route. */
export const nativeCalendar = scoped('calendar');
/** Single scenario row; sole read authority is the native owner route. */
export const nativeScenario = single('scenario');
/** Single scoped recommendation row; sole read authority is the native owner route. */
export const nativeScopedRecommendation = single('scoped-recommendation');
/** Single comparison row; sole read authority is the native owner route. */
export const nativeComparison = single('comparison');
/** Executive shortage evidence row; sole read authority is the native owner route. */
export const nativeExecutiveShortage = async (
  recordId: string,
  companyId: string,
  branchId: string | null,
  from: string,
  to: string,
  actor?: ScopedActor,
): Promise<Row | null> =>
  single('executive-shortage')({ id: recordId, from, to }, companyId, branchId, actor);
/** Executive recommendation evidence row; eligibility is a closed owner fact. */
export const nativeExecutiveRecommendation = async (
  recordId: string,
  companyId: string,
  branchId: string | null,
  from: string,
  to: string,
  eligibility: 'proposed' | 'capacity',
  actor?: ScopedActor,
): Promise<Row | null> =>
  single('executive-recommendation')(
    { id: recordId, from, to, eligibility },
    companyId,
    branchId,
    actor,
  );
