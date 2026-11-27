import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';

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

export type AudienceNode = {
  id: string;
  kind: string;
  branch_id: string | null;
};

export type UnitReference = {
  id: string;
  name: string;
  kind: string;
  branchId: string | null;
};

export type AttachmentNodeFact = {
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  version: number;
  kind: string;
};

export type NodeReferences = {
  node: AudienceNode | null;
  units: UnitReference[];
  attachment: AttachmentNodeFact | null;
};

export type NodeReferenceScope = Readonly<{ companyId: string; nodeId?: string | null }>;

export type ExecutiveEvidenceKind =
  | 'connection-scopes'
  | 'context-nodes'
  | 'context-positions'
  | 'node-evidence'
  | 'pending-evidence';

export type ExecutiveEvidenceScope = Readonly<{
  kind: ExecutiveEvidenceKind;
  companyId: string;
  values?: readonly (string | null)[];
  query?: string | null;
  limit?: number | null;
  nodeId?: string | null;
  cursorName?: string | null;
  cursorAt?: number | null;
  cursorId?: string | null;
}>;

export type ExecutiveEvidence = { rows: Record<string, unknown>[] };

const EVIDENCE_KINDS = new Set<ExecutiveEvidenceKind>([
  'connection-scopes',
  'context-nodes',
  'context-positions',
  'node-evidence',
  'pending-evidence',
]);

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
const DIRECTORY_CONFIG_ENV = 'ERPX_NATIVE_ORGANIZATION_DIRECTORY_CONFIG';
const SHARED_CONFIG_ENV = 'ERPX_NATIVE_ORGANIZATION_CONFIG';

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

const scopeId = (value: unknown): value is string =>
  identifier(value) && /^[A-Za-z0-9_.:-]+$/.test(value);

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

function audienceOf(value: unknown): AudienceNode {
  if (
    !record(value) ||
    !exactKeys(value, ['id', 'kind', 'branch_id']) ||
    !identifier(value.id) ||
    !identifier(value.kind) ||
    !nullableIdentifier(value.branch_id)
  )
    return invalid();
  return { id: value.id, kind: value.kind, branch_id: value.branch_id };
}

function unitOf(value: unknown): UnitReference {
  if (
    !record(value) ||
    !exactKeys(value, ['id', 'name', 'kind', 'branchId']) ||
    !identifier(value.id) ||
    !identifier(value.name) ||
    !identifier(value.kind) ||
    !nullableIdentifier(value.branchId)
  )
    return invalid();
  return { id: value.id, name: value.name, kind: value.kind, branchId: value.branchId };
}

function attachmentOf(value: unknown): AttachmentNodeFact {
  if (
    !record(value) ||
    !exactKeys(value, ['company_id', 'branch_id', 'department_id', 'version', 'kind']) ||
    !identifier(value.company_id) ||
    !nullableIdentifier(value.branch_id) ||
    !nullableIdentifier(value.department_id) ||
    !integer(value.version) ||
    value.version < 1 ||
    !identifier(value.kind)
  )
    return invalid();
  return {
    company_id: value.company_id,
    branch_id: value.branch_id,
    department_id: value.department_id,
    version: value.version,
    kind: value.kind,
  };
}

/** Strict projection of the native node-reference payload. */
export function validateNodeReferences(value: unknown): NodeReferences {
  if (!record(value) || !exactKeys(value, ['data'])) return invalid();
  const data = value.data;
  if (!record(data) || !exactKeys(data, ['node', 'units', 'attachment'])) return invalid();
  const units = rows(data.units, unitOf);
  if (new Set(units.map(unit => unit.id)).size !== units.length) return invalid();
  return {
    node: data.node === null ? null : audienceOf(data.node),
    units,
    attachment: data.attachment === null ? null : attachmentOf(data.attachment),
  };
}

async function request(
  target: DirectoryTarget,
  source: Headers | undefined,
  path: string,
  body?: unknown,
): Promise<unknown> {
  // Lane 23 alone accepts a directory-specific config so a single app can
  // override the inherited shared ERPX native config without changing it.
  const raw = process.env[DIRECTORY_CONFIG_ENV] ?? process.env[SHARED_CONFIG_ENV];
  const config = configuration(raw);
  const holding = holdingFor(target, config);
  const from = headersLike(target) ? target : (source ?? new Headers());
  const companyId = record(target) && typeof target.companyId === 'string' ? target.companyId : undefined;
  const headers = requestHeaders(from, holding, companyId);
  if (body !== undefined) headers.set('content-type', 'application/json');
  const url = new URL(
    `${config.base.origin}/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/organization${path}`,
  );
  let response: Response;
  try {
    response = await nativeRequest(
      url,
      {
        method: body === undefined ? 'GET' : 'POST',
        headers: Object.fromEntries(headers.entries()),
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        signal: AbortSignal.timeout(config.timeout),
      },
      config.timeout,
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
  return payload;
}

async function read(target: DirectoryTarget, source?: Headers): Promise<DirectoryHierarchy> {
  return validateDirectoryHierarchy(await request(target, source, '/directory-hierarchy'));
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

/** Bounded node-reference read shared by Collaboration, IAM and Files ports. */
export async function nativeNodeReferences(
  target: DirectoryTarget,
  scope: NodeReferenceScope,
  source?: Headers,
): Promise<NodeReferences> {
  if (!scopeId(scope.companyId) || (scope.nodeId != null && !scopeId(scope.nodeId))) return invalid();
  const query = new URLSearchParams({ companyId: scope.companyId });
  if (scope.nodeId) query.set('nodeId', scope.nodeId);
  return validateNodeReferences(await request(target, source, `/node-references?${query.toString()}`));
}

/** Strict projection of one native executive evidence payload. */
export function validateExecutiveEvidence(value: unknown): ExecutiveEvidence {
  if (!record(value) || !exactKeys(value, ['data'])) return invalid();
  const data = value.data;
  if (!record(data) || !exactKeys(data, ['rows'])) return invalid();
  const rows = data.rows;
  if (!Array.isArray(rows) || rows.length > MAX_ROWS) return invalid();
  return {
    rows: rows.map(row => {
      if (!record(row) || !identifier(row.id)) return invalid();
      return row;
    }),
  };
}

/** Bounded executive evidence read owned by the Rust organization crate. */
export async function nativeExecutiveEvidence(
  target: DirectoryTarget,
  scope: ExecutiveEvidenceScope,
  source?: Headers,
): Promise<ExecutiveEvidence> {
  if (!EVIDENCE_KINDS.has(scope.kind) || !scopeId(scope.companyId)) return invalid();
  if (scope.values !== undefined && (scope.values.length > 8 || scope.values.some(value => value !== null && !identifier(value)))) return invalid();
  if (scope.query != null && scope.query.length > 0 && !identifier(scope.query)) return invalid();
  if (scope.limit != null && (!integer(scope.limit) || scope.limit < 1 || scope.limit > 500)) return invalid();
  if (scope.nodeId != null && !scopeId(scope.nodeId)) return invalid();
  if (scope.cursorName != null && !identifier(scope.cursorName)) return invalid();
  if (scope.cursorAt != null && !integer(scope.cursorAt)) return invalid();
  if (scope.cursorId != null && !scopeId(scope.cursorId)) return invalid();
  const body: Record<string, unknown> = { kind: scope.kind, companyId: scope.companyId };
  if (scope.values !== undefined) body.values = [...scope.values];
  if (scope.query) body.query = scope.query;
  if (scope.limit != null) body.limit = scope.limit;
  if (scope.nodeId != null) body.nodeId = scope.nodeId;
  if (scope.cursorName != null) body.cursorName = scope.cursorName;
  if (scope.cursorAt != null) body.cursorAt = scope.cursorAt;
  if (scope.cursorId != null) body.cursorId = scope.cursorId;
  return validateExecutiveEvidence(await request(target, source, '/executive-evidence', body));
}
