import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict private transport for the native Operations lane-28 staff-level
 * acceptance facts router. The axum owner executes the SQL, applies the
 * aggregation and returns the exact response shape; this bridge emits no SQL,
 * accepts no SQL from a caller and fails closed when the native owner is
 * unavailable or its payload is malformed. There is no TypeScript fallback.
 *
 * Holdings are the exact registered development targets. A Holding is selected
 * by the caller's WorkContext (holdingId + holdingSlug), or by an explicit
 * company identity when no context is available. There is deliberately no
 * first-holding default: an unresolvable context fails closed.
 */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

/** Minimal WorkContext shape required for Holding selection. */
export type Lane28WorkContext = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

export type NativeStaffLevel = Readonly<{ id: string; rank: number; levelNumber: number }>;
export type NativeStaffLevelFacts = Readonly<{
  levels: readonly NativeStaffLevel[];
  activeCount: number;
}>;
export type NativeLevelHistoryRow = Readonly<{
  created_at: number;
  reason: string;
  level_name: string | null;
  previous_level_name: string | null;
}>;

const ENV = 'ERPX_NATIVE_OPERATIONS_LANE28_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const reject = (status: number, code: string, message: string): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (
  code = 'OPERATIONS_LANE28_UNAVAILABLE',
  message = 'The native Operations staff-level read service is unavailable.',
): never => reject(503, code, message);
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const identifier = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 200;

function configuration(): Settings {
  const raw = process.env[ENV];
  if (!raw) return unavailable('OPERATIONS_LANE28_CONFIGURATION_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable('OPERATIONS_LANE28_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable('OPERATIONS_LANE28_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable('OPERATIONS_LANE28_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable('OPERATIONS_LANE28_CONFIGURATION_INVALID');
  const holdings: Holding[] = [];
  const seen = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      typeof item.origin !== 'string' ||
      typeof item.host !== 'string' ||
      !identifier(item.holdingId) ||
      typeof item.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId))
    )
      return unavailable('OPERATIONS_LANE28_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable('OPERATIONS_LANE28_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.host !== item.host ||
      origin.username ||
      origin.password
    )
      return unavailable('OPERATIONS_LANE28_CONFIGURATION_INVALID');
    const key = `${item.holdingId}\u0000${item.holdingSlug}`;
    if (seen.has(key)) return unavailable('OPERATIONS_LANE28_CONFIGURATION_INVALID');
    seen.add(key);
    holdings.push({
      origin: origin.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      companyId: item.companyId as string | undefined,
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Select the exact registered Holding. WorkContext wins; an explicit company
 *  identity is the only fallback. No first-holding default exists. */
function selectHolding(config: Settings, actor?: Lane28WorkContext, companyId?: string): Holding {
  if (actor) {
    const target = config.holdings.find(
      (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
    );
    if (!target) return reject(409, 'WORK_CONTEXT_CHANGED', 'Choose a current Operations Holding.');
    if (actor.companyId && target.companyId && actor.companyId !== target.companyId)
      return reject(409, 'WORK_CONTEXT_CHANGED', 'The Operations company context changed.');
    if (companyId && target.companyId && companyId !== target.companyId)
      return reject(409, 'WORK_CONTEXT_CHANGED', 'The Operations company scope changed.');
    return target;
  }
  if (companyId) {
    const target = config.holdings.find((holding) => holding.companyId === companyId);
    if (!target) return reject(409, 'WORK_CONTEXT_CHANGED', 'Use a registered Holding for this company.');
    return target;
  }
  return reject(409, 'WORK_CONTEXT_REQUIRED', 'A WorkContext or company identity is required.');
}

async function call(
  config: Settings,
  target: Holding,
  path: string,
  headers: Readonly<Record<string, string>>,
): Promise<unknown> {
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/operations/lane28/${path}`,
    config.base,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers: {
        host: target.host,
        origin: target.origin,
        accept: 'application/json',
        'cache-control': 'no-cache',
        ...headers,
      },
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        typeof error?.code === 'string' ? error.code : 'OPERATIONS_LANE28_READ_FAILED',
        typeof error?.message === 'string' ? error.message : 'The native Operations read was rejected.',
      );
    return unavailable();
  }
  return await response.json().catch((): unknown => null);
}

function facts(value: unknown): NativeStaffLevelFacts {
  if (!record(value)) return unavailable('OPERATIONS_LANE28_RESPONSE_INVALID');
  const data = value.data;
  if (!record(data) || !Array.isArray(data.levels) || !Number.isSafeInteger(data.activeCount))
    return unavailable('OPERATIONS_LANE28_RESPONSE_INVALID');
  for (const level of data.levels) {
    if (
      !record(level) ||
      !identifier(level.id) ||
      typeof level.rank !== 'number' ||
      !Number.isSafeInteger(level.rank) ||
      typeof level.levelNumber !== 'number' ||
      !Number.isSafeInteger(level.levelNumber)
    )
      return unavailable('OPERATIONS_LANE28_RESPONSE_INVALID');
  }
  return {
    levels: data.levels.map((level: Record<string, unknown>) => ({
      id: level.id as string,
      rank: level.rank as number,
      levelNumber: level.levelNumber as number,
    })),
    activeCount: data.activeCount as number,
  };
}

/** Read the owner-local staff-level facts on the native route. The Holding is
 *  selected from the caller's WorkContext; no SQL leaves this transport. */
export async function nativeLane28StaffLevels(
  companyId: string,
  actor?: Lane28WorkContext,
): Promise<NativeStaffLevelFacts> {
  if (!identifier(companyId)) return unavailable('OPERATIONS_LANE28_COMPANY_INVALID', 'Use one bounded Operations company identity.');
  const config = configuration();
  const target = selectHolding(config, actor, companyId);
  const payload = await call(config, target, 'staff-levels', { 'x-erpx-context-company': companyId });
  return facts(payload);
}

/** Read one engagement's staff-level history on the native owner. Rows keep
 *  the retired snake_case keys so the People consumer is unchanged. The caller
 *  must supply a WorkContext (or a company identity) for Holding selection. */
export async function nativeLane28LevelHistory(
  engagementId: string,
  actor?: Lane28WorkContext,
  companyId?: string,
): Promise<readonly NativeLevelHistoryRow[]> {
  if (!identifier(engagementId)) return unavailable('OPERATIONS_LANE28_ENGAGEMENT_INVALID', 'Use one bounded Operations engagement identity.');
  const config = configuration();
  const target = selectHolding(config, actor, companyId);
  const payload = await call(config, target, 'staff-level-history', {
    'x-erpx-context-engagement': engagementId,
  });
  if (!record(payload) || !record(payload.data) || !Array.isArray(payload.data.history))
    return unavailable('OPERATIONS_LANE28_RESPONSE_INVALID');
  for (const row of payload.data.history) {
    if (
      !record(row) ||
      typeof row.created_at !== 'number' ||
      !Number.isSafeInteger(row.created_at) ||
      typeof row.reason !== 'string' ||
      (row.level_name !== null && typeof row.level_name !== 'string') ||
      (row.previous_level_name !== null && typeof row.previous_level_name !== 'string')
    )
      return unavailable('OPERATIONS_LANE28_RESPONSE_INVALID');
  }
  return payload.data.history as readonly NativeLevelHistoryRow[];
}

/** True when the native lane-28 owner transport is configured. */
export function nativeLane28Configured(environment: Readonly<Record<string, string | undefined>> = process.env): boolean {
  return environment[ENV] !== undefined;
}
