import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlResult } from '@erpx/platform-runtime-server/public/runtime/types';

/**
 * Thin, fail-closed transport for the native Commerce lane 14 workflow
 * order reads. The axum owner owns the SQL, the Company scope, the projection
 * and the response shape; this client emits no SQL and keeps no fallback
 * reader. Missing, malformed or failing native replies fail closed with 503.
 */
type Config = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

export const NATIVE_LANE14_CONFIG_ENV = 'ERPX_NATIVE_COMMERCE_LANE14_CONFIG';

const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const UNAVAILABLE = 'COMMERCE_LANE14_READ_UNAVAILABLE';
const CONFIG_INVALID = 'COMMERCE_LANE14_READ_CONFIG_INVALID';
const RESPONSE_INVALID = 'COMMERCE_LANE14_READ_RESPONSE_INVALID';

const unavailable = (code = UNAVAILABLE): never => {
  throw new ApiError(503, code, 'The native Commerce workflow order read service is unavailable.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

/** Selector presence only; an absent configuration never probes the service. */
export function nativeLane14Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[NATIVE_LANE14_CONFIG_ENV] !== undefined;
}

function config(): Config {
  const raw = process.env[NATIVE_LANE14_CONFIG_ENV];
  if (raw === undefined) return unavailable(CONFIG_INVALID);
  let value: {
    privateBaseUrl?: unknown;
    host?: unknown;
    holdingSlug?: unknown;
    origin?: unknown;
    timeoutMs?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    return unavailable(CONFIG_INVALID);
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    return unavailable(CONFIG_INVALID);
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    base.username ||
    base.password ||
    base.origin !== String(value.privateBaseUrl) ||
    !base.port ||
    RESERVED_PORTS.has(Number(base.port)) ||
    (Number(base.port) >= 3200 && Number(base.port) <= 3299) ||
    origin.protocol !== 'https:' ||
    origin.username ||
    origin.password ||
    origin.origin !== String(value.origin) ||
    typeof value.host !== 'string' ||
    value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000
  )
    return unavailable(CONFIG_INVALID);
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

/** GET with an explicit Host header; fetch forbids overriding Host. */
function get(url: URL, headers: Record<string, string>, timeoutMs: number): Promise<{ status: number; value: unknown }> {
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
          const text = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try {
            value = text ? JSON.parse(text) : null;
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

/** Pinned projection of `commerce_orders`; the Rust owner list is authoritative. */
export const LANE14_COLUMNS = [
  'id', 'company_id', 'branch_id', 'department_id', 'kind', 'number', 'party_id',
  'warehouse_id', 'currency', 'order_date', 'due_date', 'note', 'state',
  'subtotal_minor', 'tax_minor', 'source_id', 'version', 'created_by', 'approved_by',
  'created_at', 'updated_at', 'price_list_id', 'price_list_version', 'discount_reason',
  'discount_approval_required', 'discount_approved_by', 'discount_approved_at',
] as const;

export type CommerceLane14Row = Record<string, string | number | null>;

const validId = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && value.length <= 200 &&
  /^[A-Za-z0-9._:-]+$/.test(value);

function isRow(value: unknown): value is CommerceLane14Row {
  if (!record(value)) return false;
  const keys = Object.keys(value);
  if (keys.length !== LANE14_COLUMNS.length) return false;
  const allowed = new Set<string>(LANE14_COLUMNS);
  return keys.every(key => allowed.has(key)) && Object.values(value).every(cell =>
    cell === null || typeof cell === 'string' ||
    (typeof cell === 'number' && Number.isFinite(cell) && Number.isSafeInteger(cell)));
}

function isRows(value: unknown, max: number): value is CommerceLane14Row[] {
  return Array.isArray(value) && value.length <= max && value.every(isRow);
}

function result<T extends CommerceLane14Row>(rows: T[]): SqlResult<T> {
  return { results: rows, success: true, meta: { changes: 0, duration: 0 } };
}

async function read(
  route: 'workflow-document' | 'workflow-document-page' | 'workflow-reference-rows',
  companyId: string,
  recordId: string | undefined,
): Promise<{ status: number; value: unknown }> {
  if (!validId(companyId) || (recordId !== undefined && !validId(recordId)))
    return unavailable(RESPONSE_INVALID);
  const cfg = config();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/lane14/${route}`,
    cfg.base,
  );
  const headers: Record<string, string> = {
    host: cfg.host,
    origin: cfg.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': companyId,
  };
  if (recordId !== undefined) headers['x-erpx-context-record'] = recordId;
  try {
    return await get(url, headers, cfg.timeoutMs);
  } catch {
    return unavailable();
  }
}

function failure(status: number, value: unknown): never {
  const error = record(value) && record(value.error) ? value.error : {};
  if (status >= 400 && status < 500) {
    throw new ApiError(
      status,
      typeof error.code === 'string' && error.code.length <= 100 ? error.code : 'COMMERCE_LANE14_READ_FAILED',
      typeof error.message === 'string' && error.message.length <= 300
        ? error.message
        : 'The native Commerce workflow order read was rejected.',
    );
  }
  return unavailable();
}

/** Single owner row, or null when the order is absent. */
export async function nativeLane14Document(
  companyId: string,
  orderId: string,
): Promise<CommerceLane14Row | null> {
  const response = await read('workflow-document', companyId, orderId);
  if (response.status !== 200) return failure(response.status, response.value);
  if (!record(response.value) || !('row' in response.value)) return unavailable(RESPONSE_INVALID);
  const row = response.value.row;
  if (row === null || row === undefined) return null;
  if (!isRow(row)) return unavailable(RESPONSE_INVALID);
  return row;
}

/** Cursor page; mirrors the retired `.all()` result envelope. */
export async function nativeLane14Page(
  companyId: string,
  after: string,
): Promise<SqlResult<CommerceLane14Row>> {
  const response = await read('workflow-document-page', companyId, after);
  if (response.status !== 200) return failure(response.status, response.value);
  if (!record(response.value) || !isRows(response.value.rows, 100)) return unavailable(RESPONSE_INVALID);
  return result(response.value.rows);
}

/** Company-scoped reference rows; mirrors the retired `.all()` envelope. */
export async function nativeLane14ReferenceRows(
  companyId: string,
): Promise<SqlResult<CommerceLane14Row>> {
  const response = await read('workflow-reference-rows', companyId, undefined);
  if (response.status !== 200) return failure(response.status, response.value);
  if (!record(response.value) || !isRows(response.value.rows, 10_000)) return unavailable(RESPONSE_INVALID);
  return result(response.value.rows);
}
