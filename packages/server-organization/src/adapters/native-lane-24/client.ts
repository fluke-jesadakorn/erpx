import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict private transport for the native Organization lane-24 read router.
 * The axum owner owns the SQL, the Company-scoped bind, the projection and the
 * response shape. This bridge emits no SQL, holds no fallback and never reads
 * locally: an absent config, an unregistered Holding, a failed request or an
 * unexpected payload all fail closed.
 *
 * Every registered Holding is supported. The target is selected from the
 * caller's WorkContext (holdingId + holdingSlug, optional origin/companyId),
 * or from the exact browser origin/host for legacy fact callers. There is no
 * first-Holding default; an unknown target never reaches the private service.
 */
export const LANE24_EVAL_POSITION_BY_LEVEL = 'eval_position_by_level';
export const LANE24_EVAL_LEVEL_ACTIVE = 'eval_level_active';
export const LANE24_EVAL_WORKSPACE_POSITIONS = 'eval_workspace_positions';
export const LANE24_EVAL_WORKSPACE_LEVELS = 'eval_workspace_levels';
export const LANE24_EVAL_MARKET_LEVELS = 'eval_market_levels';
export const LANE24_ORG_ACCESS_NODES = 'org_access_nodes';

export type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

/** WorkContext-derived target, or an explicit registered origin/host. */
export type Lane24Actor = Readonly<{
  holdingId?: string | null;
  holdingSlug?: string | null;
  companyId?: string | null;
  origin?: string | null;
  host?: string | null;
}>;

/** Caller WorkContext, explicit registered host/origin, or inbound browser headers. */
export type Lane24Target =
  | Headers
  | Readonly<{ holdingId?: string | null; holdingSlug?: string | null; origin?: string | null; host?: string | null; companyId?: string | null }>;

type Config = Readonly<{
  base: URL;
  holdings: readonly Holding[];
  byHost: ReadonlyMap<string, Holding>;
  byOrigin: ReadonlyMap<string, Holding>;
  byId: ReadonlyMap<string, Holding>;
  timeout: number;
}>;

const ENV = 'ERPX_NATIVE_ORGANIZATION_LANE24_CONFIG';
const MAX_HOLDINGS = 64;
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const KINDS = new Set<string>([
  LANE24_EVAL_POSITION_BY_LEVEL,
  LANE24_EVAL_LEVEL_ACTIVE,
  LANE24_EVAL_WORKSPACE_POSITIONS,
  LANE24_EVAL_WORKSPACE_LEVELS,
  LANE24_EVAL_MARKET_LEVELS,
  LANE24_ORG_ACCESS_NODES,
]);
const CONTEXT_HEADERS = [
  'cookie',
  'x-erpx-context-assignment',
  'x-erpx-context-revision',
  'x-erpx-context-assignment-version',
  'x-erpx-workspace-branch',
  'x-erpx-workspace-department',
] as const;

const unavailable = (): never => {
  throw new ApiError(503, 'ORGANIZATION_LANE24_UNAVAILABLE', 'The native Organization lane-24 read is unavailable.');
};
const configurationInvalid = (): never => {
  throw new ApiError(503, 'ORGANIZATION_LANE24_CONFIG_INVALID', 'Organization lane-24 transport is misconfigured.');
};
const targetInvalid = (): never => {
  throw new ApiError(503, 'ORGANIZATION_LANE24_TARGET_INVALID', 'The native Organization lane-24 Holding is not registered.');
};
const invalid = (): never => {
  throw new ApiError(503, 'ORGANIZATION_LANE24_INVALID', 'The native Organization lane-24 read was invalid.');
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const headersLike = (value: unknown): value is Headers =>
  typeof (value as { get?: unknown } | null)?.get === 'function';
const identifier = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 200;
const originOf = (value: string): string | null => {
  try {
    const url = new URL(value);
    return url.protocol === 'https:' && url.origin === value && !url.username && !url.password
      ? url.origin
      : null;
  } catch {
    return null;
  }
};

/** Parse and strictly validate the 1..64 holding transport configuration. */
export function configuration(raw: string | undefined): Config {
  if (raw === undefined) return unavailable();
  let value: { privateBaseUrl?: unknown; timeoutMs?: unknown; holdings?: unknown };
  try { value = JSON.parse(raw) as typeof value; } catch { return configurationInvalid(); }
  let base: URL;
  try { base = new URL(String(value.privateBaseUrl)); } catch { return configurationInvalid(); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username || base.password ||
    value.privateBaseUrl !== base.origin ||
    typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 || value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 || value.holdings.length > MAX_HOLDINGS
  ) return configurationInvalid();

  const holdings: Holding[] = [];
  const byHost = new Map<string, Holding>();
  const byOrigin = new Map<string, Holding>();
  const byId = new Map<string, Holding>();
  for (const item of value.holdings as unknown[]) {
    if (
      !record(item) ||
      typeof item.origin !== 'string' ||
      typeof item.host !== 'string' ||
      typeof item.holdingId !== 'string' ||
      typeof item.holdingSlug !== 'string' ||
      (item.companyId !== undefined && !identifier(item.companyId))
    ) return configurationInvalid();
    const origin = originOf(item.origin);
    if (
      origin === null ||
      item.host !== new URL(item.origin).host ||
      !identifier(item.holdingId) ||
      !/^[A-Za-z0-9_.:-]{1,200}$/.test(item.holdingId) ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug) ||
      byHost.has(item.host) || byOrigin.has(origin) || byId.has(item.holdingId)
    ) return configurationInvalid();
    const holding: Holding = {
      origin: item.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      ...(item.companyId === undefined ? {} : { companyId: item.companyId }),
    };
    holdings.push(holding);
    byHost.set(holding.host, holding);
    byOrigin.set(origin, holding);
    byId.set(holding.holdingId, holding);
  }
  return { base, holdings, byHost, byOrigin, byId, timeout: value.timeoutMs };
}

/** Resolve exactly one registered Holding. No first-holding default. */
export function holdingFor(target: Lane24Target, config: Config): Holding {
  if (headersLike(target)) {
    const fromOrigin = target.get('origin');
    if (fromOrigin) {
      const origin = originOf(fromOrigin);
      const holding = origin === null ? undefined : config.byOrigin.get(origin);
      if (!holding) return targetInvalid();
      return holding;
    }
    const host = target.get('host');
    const holding = host ? config.byHost.get(host.split(':')[0]!) : undefined;
    if (!holding) return targetInvalid();
    return holding;
  }
  if (record(target)) {
    const { holdingId, holdingSlug, origin, host, companyId } = target;
    let holding: Holding | undefined;
    if (identifier(holdingId) && identifier(holdingSlug)) {
      holding = config.holdings.find(
        item => item.holdingId === holdingId && item.holdingSlug === holdingSlug,
      );
      if (!holding) return targetInvalid();
    }
    if (typeof origin === 'string' && origin.length > 0) {
      const key = originOf(origin);
      const byOrigin = key === null ? undefined : config.byOrigin.get(key);
      if (!byOrigin) return targetInvalid();
      if (holding && holding !== byOrigin) return targetInvalid();
      holding = byOrigin;
    }
    if (typeof host === 'string' && host.length > 0) {
      const byHost = config.byHost.get(host.split(':')[0]!);
      if (!byHost) return targetInvalid();
      if (holding && holding !== byHost) return targetInvalid();
      holding = byHost;
    }
    if (holding) {
      if (identifier(companyId)) {
        if (holding.companyId !== undefined && holding.companyId !== companyId) return targetInvalid();
      }
      return holding;
    }
  }
  return targetInvalid();
}

function requestHeaders(source: Headers, holding: Holding, companyId: string): Headers {
  const headers = new Headers({ host: holding.host, origin: holding.origin, accept: 'application/json' });
  for (const name of CONTEXT_HEADERS) {
    const value = source.get(name);
    if (value && new TextEncoder().encode(value).length <= 8192) headers.set(name, value);
  }
  headers.set('x-erpx-context-company', companyId);
  return headers;
}

const stringRows = (value: unknown, keys: readonly string[]): Record<string, string>[] | null => {
  if (!Array.isArray(value)) return null;
  const rows: Record<string, string>[] = [];
  for (const item of value) {
    if (!record(item)) return null;
    const row: Record<string, string> = {};
    for (const key of keys) {
      if (typeof item[key] !== 'string') return null;
      row[key] = item[key] as string;
    }
    rows.push(row);
  }
  return rows;
};

/** Strict projection of one owner payload. Exported for contract tests. */
export function validateFacts(value: unknown, kind: string): Record<string, unknown> {
  if (!record(value) || Object.keys(value).length !== 1 || !record(value.data)) return invalid();
  const data = value.data;
  if (data.kind !== kind) return invalid();
  const keys = Object.keys(data).filter(key => key !== 'kind');
  switch (kind) {
    case LANE24_EVAL_POSITION_BY_LEVEL:
      if (keys.length !== 1 || keys[0] !== 'row') return invalid();
      if (data.row !== null && (!record(data.row) || typeof data.row.id !== 'string')) return invalid();
      break;
    case LANE24_EVAL_LEVEL_ACTIVE:
      if (keys.length !== 1 || keys[0] !== 'exists' || typeof data.exists !== 'boolean') return invalid();
      break;
    case LANE24_EVAL_WORKSPACE_POSITIONS:
      if (keys.length !== 1 || keys[0] !== 'rows' ||
          stringRows(data.rows, ['id', 'title', 'level_id']) === null) return invalid();
      break;
    case LANE24_EVAL_WORKSPACE_LEVELS:
      if (keys.length !== 1 || keys[0] !== 'rows' ||
          stringRows(data.rows, ['id', 'name']) === null) return invalid();
      break;
    case LANE24_EVAL_MARKET_LEVELS:
      if (keys.length !== 1 || keys[0] !== 'rows' ||
          stringRows(data.rows, ['id']) === null) return invalid();
      break;
    case LANE24_ORG_ACCESS_NODES:
      if (keys.length !== 1 || keys[0] !== 'rows' || !Array.isArray(data.rows)) return invalid();
      for (const item of data.rows) {
        if (!record(item) ||
            typeof item.id !== 'string' ||
            (item.parent_id !== null && typeof item.parent_id !== 'string') ||
            typeof item.kind !== 'string' ||
            (item.display_name !== null && typeof item.display_name !== 'string') ||
            typeof item.status !== 'string') return invalid();
      }
      break;
    default:
      return invalid();
  }
  return data;
}

async function read(target: Lane24Target | undefined, companyId: string, kind: string, values: readonly string[]): Promise<Record<string, unknown>> {
  if (!identifier(companyId) || !KINDS.has(kind) || values.some(value => !identifier(value))) return invalid();
  const expected = kind === LANE24_EVAL_POSITION_BY_LEVEL ? 2
    : kind === LANE24_EVAL_LEVEL_ACTIVE ? 1 : 0;
  if (values.length !== expected) return invalid();
  const config = configuration(process.env[ENV]);
  const holding = holdingFor(target ?? {}, config);
  if (identifier(holding.companyId) && holding.companyId !== companyId) return targetInvalid();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/organization/lane24/evaluation-facts`,
    config.base.origin,
  );
  const source = headersLike(target) ? target : new Headers();
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: requestHeaders(source, holding, companyId),
      body: JSON.stringify({ kind, values }),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeout),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  let payload: unknown;
  try { payload = await response.json(); } catch { return invalid(); }
  return validateFacts(payload, kind);
}

export async function nativeLane24PositionByLevel(
  target: Lane24Target | undefined,
  companyId: string,
  positionId: string,
  levelId: string,
): Promise<{ id: string } | null> {
  const data = await read(target, companyId, LANE24_EVAL_POSITION_BY_LEVEL, [positionId, levelId]);
  return data.row as { id: string } | null;
}

/** Existence of an active staff level, preserving the retired `.first()` truthiness. */
export async function nativeLane24LevelActive(
  target: Lane24Target | undefined,
  companyId: string,
  levelId: string,
): Promise<{ one: number } | null> {
  const data = await read(target, companyId, LANE24_EVAL_LEVEL_ACTIVE, [levelId]);
  return data.exists === true ? { one: 1 } : null;
}

export async function nativeLane24WorkspacePositions(
  target: Lane24Target | undefined,
  companyId: string,
): Promise<Array<{ id: string; title: string; level_id: string }>> {
  const data = await read(target, companyId, LANE24_EVAL_WORKSPACE_POSITIONS, []);
  return data.rows as Array<{ id: string; title: string; level_id: string }>;
}

export async function nativeLane24WorkspaceLevels(
  target: Lane24Target | undefined,
  companyId: string,
): Promise<Array<{ id: string; name: string }>> {
  const data = await read(target, companyId, LANE24_EVAL_WORKSPACE_LEVELS, []);
  return data.rows as Array<{ id: string; name: string }>;
}

export async function nativeLane24MarketLevels(
  target: Lane24Target | undefined,
  companyId: string,
): Promise<Array<{ id: string }>> {
  const data = await read(target, companyId, LANE24_EVAL_MARKET_LEVELS, []);
  return data.rows as Array<{ id: string }>;
}

export async function nativeLane24AccessNodes(
  target: Lane24Target | undefined,
  companyId: string,
): Promise<Array<{ id: string; parent_id: string | null; kind: string; display_name: string | null; status: string }>> {
  const data = await read(target, companyId, LANE24_ORG_ACCESS_NODES, []);
  return data.rows as Array<{ id: string; parent_id: string | null; kind: string; display_name: string | null; status: string }>;
}
