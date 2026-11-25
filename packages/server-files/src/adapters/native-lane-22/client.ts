import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict private transport for the lane 22 native Files work-evidence read.
 *
 * The axum owner EXECUTES both statements with sqlx and returns the complete
 * row projection. This bridge emits no SQL, accepts no SQL from a caller and
 * fails closed with 503 when the owner is unavailable, the registered Holding
 * is unknown or a response is malformed. There is no TypeScript fallback and
 * no default Holding: every request must name a registered target from the
 * caller's WorkContext.
 */
export type WorkEvidenceHolding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
export type WorkEvidenceConfig = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly WorkEvidenceHolding[];
}>;
export type WorkEvidenceActor = {
  holdingId?: string;
  holdingSlug?: string;
  origin?: string;
  companyId: string;
};
export type WorkEvidenceKind = 'sets' | 'legacy';
export type WorkEvidenceTransport = (
  url: URL,
  headers: Record<string, string>,
  timeoutMs: number,
) => Promise<{ status: number; value: unknown }>;
export type WorkEvidenceRow = {
  id: string;
  set_id: string | null;
  purpose_key: string;
  original_name: string;
  bytes: number | null;
  media_type: string | null;
  source_state: string | null;
  link_state: string;
  preview_available: boolean;
  created_at: number;
};

type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly WorkEvidenceHolding[];
}>;
type Environment = Readonly<Record<string, string | undefined>>;

const ENV = 'ERPX_NATIVE_FILES_CONFIG';
const RESERVED = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (
  code = 'NATIVE_WORK_EVIDENCE_UNAVAILABLE',
  message = 'The native Files work-evidence service is unavailable.',
): never => {
  throw new ApiError(503, code, message);
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown, max = 200): value is string =>
  text(value, max) && !/[\u0000-\u001f/?#\\]/.test(value);
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value);
const hostname = (value: unknown): value is string =>
  text(value, 253) && /^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$/.test(value);

export function nativeWorkEvidenceConfigured(
  environment: Environment = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

/** Parse and validate 1..64 distinct registered Holdings. */
export function parseWorkEvidenceConfig(value: unknown): Settings {
  if (
    !object(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable('NATIVE_WORK_EVIDENCE_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return unavailable('NATIVE_WORK_EVIDENCE_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable('NATIVE_WORK_EVIDENCE_CONFIGURATION_INVALID');

  const holdings: WorkEvidenceHolding[] = [];
  const ids = new Set<string>();
  const origins = new Set<string>();
  const targets = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !hostname(item.host) ||
      !identifier(item.holdingId) ||
      !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId))
    )
      return unavailable('NATIVE_WORK_EVIDENCE_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable('NATIVE_WORK_EVIDENCE_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      // The Host header must be the exact registered Holding host.
      origin.host !== item.host ||
      ids.has(String(item.holdingId)) ||
      origins.has(origin.origin) ||
      targets.has(`${String(item.host)}\u0000${String(item.holdingSlug)}`)
    )
      return unavailable('NATIVE_WORK_EVIDENCE_CONFIGURATION_INVALID');
    ids.add(String(item.holdingId));
    origins.add(origin.origin);
    targets.add(`${String(item.host)}\u0000${String(item.holdingSlug)}`);
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Pick exactly one registered Holding. Two selectors must agree; no first-Holding default. */
function selectTarget(settings: Settings, actor: WorkEvidenceActor): WorkEvidenceHolding {
  const invalidRequest = () =>
    unavailable('NATIVE_WORK_EVIDENCE_REQUEST_INVALID', 'Use one bounded Files work-evidence request.');
  const unknown = () => unavailable('NATIVE_WORK_EVIDENCE_TARGET_UNAVAILABLE');

  let byOrigin: WorkEvidenceHolding | undefined;
  if (actor.origin !== undefined) {
    if (!text(actor.origin, 512)) return invalidRequest();
    let origin: URL;
    try {
      origin = new URL(actor.origin);
    } catch {
      return invalidRequest();
    }
    if (origin.origin !== actor.origin || origin.protocol !== 'https:') return invalidRequest();
    byOrigin = settings.holdings.find(holding => holding.origin === origin.origin);
  }

  let byIdentity: WorkEvidenceHolding | undefined;
  if (actor.holdingId !== undefined || actor.holdingSlug !== undefined) {
    if (!identifier(actor.holdingId) || !slug(actor.holdingSlug)) return invalidRequest();
    byIdentity = settings.holdings.find(
      holding => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
    );
  }

  if (!byOrigin && !byIdentity) return unknown();
  if (byOrigin && byIdentity && byOrigin !== byIdentity) return unknown();
  const target = byOrigin ?? byIdentity!;
  if (!identifier(actor.companyId)) return invalidRequest();
  if (target.companyId !== undefined && target.companyId !== actor.companyId)
    return unavailable('NATIVE_WORK_EVIDENCE_COMPANY_MISMATCH');
  return target;
}

/** GET with an explicit Host header; fetch forbids overriding Host. */
function get(
  url: URL,
  headers: Record<string, string>,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method: 'GET',
        agent,
        headers: { ...headers },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
        incoming.on('end', () => {
          agent.destroy();
          const body = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try {
            value = body ? JSON.parse(body) : null;
          } catch {
            value = null;
          }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', error => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on('error', error => {
      agent.destroy();
      reject(error);
    });
    outgoing.end();
  });
}

const nullableText = (value: unknown, max = 8192): value is string | null =>
  value === null || text(value, max);
const nullableInt = (value: unknown): value is number | null =>
  value === null || (typeof value === 'number' && Number.isSafeInteger(value));

function row(value: unknown, kind: WorkEvidenceKind): value is WorkEvidenceRow {
  if (!object(value)) return false;
  // The two retired statements project set_id differently: the sets JOIN
  // yields a non-null link.set_id, the legacy statement projects NULL::text.
  const setIdValid = kind === 'sets' ? text(value.set_id, 200) : value.set_id === null;
  return (
    setIdValid &&
    text(value.id, 200) &&
    text(value.purpose_key, 200) &&
    text(value.original_name, 8192) &&
    nullableInt(value.bytes) &&
    nullableText(value.media_type, 512) &&
    nullableText(value.source_state, 64) &&
    text(value.link_state, 64) &&
    typeof value.preview_available === 'boolean' &&
    typeof value.created_at === 'number' &&
    Number.isSafeInteger(value.created_at)
  );
}

/** Create a client with an injectable transport for contract tests. */
export function createWorkEvidenceClient(
  config: unknown,
  transport: WorkEvidenceTransport = get,
) {
  const settings = parseWorkEvidenceConfig(config);
  return {
    async read(
      actor: WorkEvidenceActor,
      _database: SqlDatabase,
      recordId: string,
      kind: WorkEvidenceKind,
      readOnly: boolean,
    ): Promise<SqlResult<WorkEvidenceRow>> {
      const invalidRequest = () =>
        unavailable('NATIVE_WORK_EVIDENCE_REQUEST_INVALID', 'Use one bounded Files work-evidence request.');
      if (!identifier(recordId)) return invalidRequest();
      if (!text(kind, 64) || (kind !== 'sets' && kind !== 'legacy')) return invalidRequest();
      const target = selectTarget(settings, actor);
      let response: { status: number; value: unknown };
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/files/work-evidence?kind=${kind}`,
        settings.base,
      );
      try {
        response = await transport(
          url,
          {
            host: target.host,
            origin: target.origin,
            accept: 'application/json',
            'cache-control': 'no-cache',
            'x-erpx-context-company': actor.companyId,
            'x-erpx-context-record': recordId,
            ...(readOnly ? { 'x-erpx-files-read-only': 'true' } : {}),
          },
          settings.timeoutMs,
        );
      } catch {
        return unavailable();
      }
      const value: unknown = response.value;
      if (response.status !== 200) {
        const error = object(value) && object(value.error) ? value.error : {};
        if (response.status >= 400 && response.status < 500)
          throw new ApiError(
            response.status,
            text(error.code, 100) ? String(error.code) : 'NATIVE_WORK_EVIDENCE_READ_FAILED',
            text(error.message, 300) ? String(error.message) : 'The native Files work-evidence read was rejected.',
          );
        return unavailable();
      }
      if (
        !object(value) ||
        value.kind !== kind ||
        !Array.isArray(value.rows) ||
        !value.rows.every(item => row(item, kind))
      )
        return unavailable('NATIVE_WORK_EVIDENCE_RESPONSE_INVALID', 'The native Files work-evidence reply was invalid.');
      const results = value.rows as WorkEvidenceRow[];
      // Read-only statements never change rows; the transport preserves the
      // SqlResult shape the owner consumer already projects with `.results`.
      return { results, success: true, meta: { changes: 0, duration: 0 } };
    },
  };
}

/** Env-bound entry point used by the owner adapter. `database` is retained for
 * call-site compatibility only; no SQL is issued on it. */
export async function readNativeWorkEvidence(
  actor: WorkEvidenceActor,
  database: SqlDatabase,
  recordId: string,
  kind: WorkEvidenceKind,
  readOnly: boolean,
  transport?: WorkEvidenceTransport,
): Promise<SqlResult<WorkEvidenceRow>> {
  const raw = process.env[ENV];
  if (!raw) return unavailable('NATIVE_WORK_EVIDENCE_CONFIGURATION_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable('NATIVE_WORK_EVIDENCE_CONFIGURATION_INVALID');
  }
  return createWorkEvidenceClient(value, transport).read(actor, database, recordId, kind, readOnly);
}
