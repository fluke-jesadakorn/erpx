import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict transport for the native Organization directory hierarchy read.
 * Every query and every projection lives in `erpx-server-organization`; this
 * module sends only a registered Holding identity and forwards browser context
 * headers. It holds no query text, no fallback and no partial projection: an
 * absent config, an unknown Holding, a failed request or a malformed payload
 * all fail closed with 503.
 */
export type DirectoryNode = {
  id: string;
  company_id: string;
  kind: 'company' | 'headquarters' | 'branch' | 'department' | 'team';
  slug: string;
  display_name: string;
  parent_id: string | null;
  branch_id: string | null;
};

export type DirectoryLevel = {
  id: string;
  company_id: string;
  code: string;
  name: string;
  level_number: number | null;
};

export type DirectoryPosition = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  title: string;
  level_id: string;
  reports_to_position_id: string | null;
  is_ceo: number;
};

export type DirectoryHierarchy = {
  nodes: DirectoryNode[];
  levels: DirectoryLevel[];
  positions: DirectoryPosition[];
};

export type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

export type DirectoryTarget =
  | Headers
  | Readonly<{ holdingId?: string; holdingSlug?: string; origin?: string; companyId?: string }>;

type Configuration = Readonly<{
  base: URL;
  holdings: readonly Holding[];
  byHost: ReadonlyMap<string, Holding>;
  byOrigin: ReadonlyMap<string, Holding>;
  byId: ReadonlyMap<string, Holding>;
  timeout: number;
}>;

const MAX_HOLDINGS = 64;
const MAX_ROWS = 20_000;
const MAX_IDENTIFIER_BYTES = 200;
const NODE_KINDS = new Set(['company', 'headquarters', 'branch', 'department', 'team']);
const CONTEXT_HEADERS = [
  'cookie',
  'origin',
  'x-erpx-context-company',
  'x-erpx-context-assignment',
  'x-erpx-context-revision',
  'x-erpx-context-assignment-version',
  'x-erpx-workspace-branch',
  'x-erpx-workspace-department',
] as const;
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (): never => {
  throw new ApiError(503, 'ORGANIZATION_DIRECTORY_HIERARCHY_UNAVAILABLE', 'The native Organization directory hierarchy is unavailable.');
};

const configurationInvalid = (): never => {
  throw new ApiError(503, 'ORGANIZATION_DIRECTORY_HIERARCHY_CONFIG_INVALID', 'Organization directory transport is misconfigured.');
};

const targetInvalid = (): never => {
  throw new ApiError(503, 'ORGANIZATION_DIRECTORY_HIERARCHY_TARGET_INVALID', 'The native Organization directory Holding is not registered.');
};

const invalid = (): never => {
  throw new ApiError(503, 'ORGANIZATION_DIRECTORY_HIERARCHY_INVALID', 'The native Organization directory hierarchy was invalid.');
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const headersLike = (value: unknown): value is Headers =>
  typeof (value as { get?: unknown } | null)?.get === 'function';

const identifier = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= MAX_IDENTIFIER_BYTES;

const nullableIdentifier = (value: unknown): value is string | null =>
  value === null || identifier(value);

const integer = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value);

const exactKeys = (value: Record<string, unknown>, keys: readonly string[]): boolean => {
  const actual = Object.keys(value);
  return actual.length === keys.length && keys.every(key => Object.prototype.hasOwnProperty.call(value, key));
};

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
export function configuration(raw: string | undefined): Configuration {
  if (raw === undefined) return unavailable();
  let value: { privateBaseUrl?: unknown; timeoutMs?: unknown; holdings?: unknown };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    return configurationInvalid();
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return configurationInvalid();
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
    value.privateBaseUrl !== base.origin ||
    !integer(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > MAX_HOLDINGS
  )
    return configurationInvalid();

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
    )
      return configurationInvalid();
    const origin = originOf(item.origin);
    if (
      origin === null ||
      item.host !== new URL(item.origin).host ||
      !identifier(item.holdingId) ||
      !/^[A-Za-z0-9_.:-]{1,200}$/.test(item.holdingId) ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug) ||
      byHost.has(item.host) ||
      byOrigin.has(origin) ||
      byId.has(item.holdingId)
    )
      return configurationInvalid();
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
  return { base, holdings, byHost, byOrigin, byId, timeout: value.timeoutMs as number };
}

/** Resolve exactly one registered Holding; there is no first-holding default. */
export function holdingFor(target: DirectoryTarget, config: Configuration): Holding {
  if (headersLike(target)) {
    const fromOrigin = target.get('origin');
    if (fromOrigin) {
      const origin = originOf(fromOrigin);
      const holding = origin === null ? undefined : config.byOrigin.get(origin);
      if (holding) return holding;
      return targetInvalid();
    }
    const host = target.get('host');
    const holding = host ? config.byHost.get(host.split(':')[0]!) : undefined;
    if (!holding) return targetInvalid();
    return holding;
  }
  if (record(target)) {
    const { holdingId, holdingSlug, origin } = target;
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
    if (holding) return holding;
  }
  return targetInvalid();
}

function requestHeaders(source: Headers, holding: Holding, companyId?: string): Headers {
  const headers = new Headers({
    host: holding.host,
    accept: 'application/json',
  });
  for (const name of CONTEXT_HEADERS) {
    const value = source.get(name);
    if (value) headers.set(name, value);
  }
  const scope = holding.companyId ?? companyId;
  if (scope && !headers.has('x-erpx-context-company')) headers.set('x-erpx-context-company', scope);
  return headers;
}

function nodeOf(value: unknown): DirectoryNode {
  if (
    !record(value) ||
    !exactKeys(value, ['id', 'company_id', 'kind', 'slug', 'display_name', 'parent_id', 'branch_id']) ||
    !identifier(value.id) ||
    !identifier(value.company_id) ||
    typeof value.kind !== 'string' ||
    !NODE_KINDS.has(value.kind) ||
    !identifier(value.slug) ||
    !identifier(value.display_name) ||
    !nullableIdentifier(value.parent_id) ||
    !nullableIdentifier(value.branch_id)
  )
    return invalid();
  return {
    id: value.id,
    company_id: value.company_id,
    kind: value.kind as DirectoryNode['kind'],
    slug: value.slug,
    display_name: value.display_name,
    parent_id: value.parent_id,
    branch_id: value.branch_id,
  };
}

function levelOf(value: unknown): DirectoryLevel {
  if (
    !record(value) ||
    !exactKeys(value, ['id', 'company_id', 'code', 'name', 'level_number']) ||
    !identifier(value.id) ||
    !identifier(value.company_id) ||
    !identifier(value.code) ||
    !identifier(value.name) ||
    !(value.level_number === null || (integer(value.level_number) && value.level_number >= 1 && value.level_number <= 6))
  )
    return invalid();
  return {
    id: value.id,
    company_id: value.company_id,
    code: value.code,
    name: value.name,
    level_number: value.level_number,
  };
}

function positionOf(value: unknown): DirectoryPosition {
  if (
    !record(value) ||
    !exactKeys(value, ['id', 'company_id', 'branch_id', 'department_id', 'title', 'level_id', 'reports_to_position_id', 'is_ceo']) ||
    !identifier(value.id) ||
    !identifier(value.company_id) ||
    !nullableIdentifier(value.branch_id) ||
    !nullableIdentifier(value.department_id) ||
    !identifier(value.title) ||
    !identifier(value.level_id) ||
    !nullableIdentifier(value.reports_to_position_id) ||
    !integer(value.is_ceo) ||
    (value.is_ceo !== 0 && value.is_ceo !== 1)
  )
    return invalid();
  return {
    id: value.id,
    company_id: value.company_id,
    branch_id: value.branch_id,
    department_id: value.department_id,
    title: value.title,
    level_id: value.level_id,
    reports_to_position_id: value.reports_to_position_id,
    is_ceo: value.is_ceo,
  };
}

const rows = <T>(value: unknown, project: (row: unknown) => T): T[] => {
  if (!Array.isArray(value) || value.length > MAX_ROWS) return invalid();
  return value.map(project);
};

/** Strict projection of one native payload. Exported for contract tests. */
export function validateDirectoryHierarchy(value: unknown): DirectoryHierarchy {
  if (!record(value) || !exactKeys(value, ['data'])) return invalid();
  const data = value.data;
  if (!record(data) || !exactKeys(data, ['nodes', 'levels', 'positions'])) return invalid();
  const facts = {
    nodes: rows(data.nodes, nodeOf),
    levels: rows(data.levels, levelOf),
    positions: rows(data.positions, positionOf),
  };
  if (new Set(facts.nodes.map(row => row.id)).size !== facts.nodes.length) return invalid();
  if (new Set(facts.levels.map(row => row.id)).size !== facts.levels.length) return invalid();
  if (new Set(facts.positions.map(row => row.id)).size !== facts.positions.length) return invalid();
  return facts;
}

async function read(target: DirectoryTarget, source?: Headers): Promise<DirectoryHierarchy> {
  const config = configuration(process.env.ERPX_NATIVE_ORGANIZATION_CONFIG);
  const holding = holdingFor(target, config);
  const from = headersLike(target) ? target : (source ?? new Headers());
  const companyId = record(target) && typeof target.companyId === 'string' ? target.companyId : undefined;
  let response: Response;
  try {
    response = await fetch(
      `${config.base.origin}/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/organization/directory-hierarchy`,
      {
        method: 'GET',
        headers: requestHeaders(from, holding, companyId),
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeout),
      },
    );
  } catch {
    return unavailable();
  }
  const payload = await response.json().catch(() => null) as unknown;
  if (!response.ok) {
    const error = record(payload) && record(payload.error) ? payload.error : {};
    throw new ApiError(
      response.status,
      typeof error.code === 'string' ? error.code : 'ORGANIZATION_DIRECTORY_HIERARCHY_FAILED',
      typeof error.message === 'string' ? error.message : 'The native Organization directory request failed.',
    );
  }
  return validateDirectoryHierarchy(payload);
}

/** One bounded native read returning all three owner-local fact arrays. */
export function nativeDirectoryHierarchy(
  target: DirectoryTarget,
  source?: Headers,
): Promise<DirectoryHierarchy> {
  return read(target, source);
}

export function nativeDirectoryNodes(target: DirectoryTarget, source?: Headers): Promise<DirectoryNode[]> {
  return read(target, source).then(facts => facts.nodes);
}

export function nativeDirectoryLevels(target: DirectoryTarget, source?: Headers): Promise<DirectoryLevel[]> {
  return read(target, source).then(facts => facts.levels);
}

export function nativeDirectoryPositions(target: DirectoryTarget, source?: Headers): Promise<DirectoryPosition[]> {
  return read(target, source).then(facts => facts.positions);
}
