import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/** Allow-listed settings read kinds resolved by the mounted owner command-plan route. */
export const RD_SETTINGS_BY_COMPANY = 'commerce_rd_settings_by_company';
export const RD_CURRENCY_BY_COMPANY = 'commerce_rd_currency_by_company';
export const RD_POSTING_MAPPING_STATE = 'commerce_rd_posting_mapping_state';

/** Allow-listed product-config read kinds resolved by the mounted owner command-plan route. */
export const RD_PRODUCT_PAGE_ROWS = 'commerce_rd_product_page_rows';
export const RD_PRODUCT_MASTER_ROWS = 'commerce_rd_product_master_rows';
export const RD_PRODUCT_ACTIVE = 'commerce_rd_product_active';
export const RD_PRODUCT_ACTIVE_OPTIONS = 'commerce_rd_product_active_options';
export const RD_PRODUCT_MASTER_DETAIL = 'commerce_rd_product_master_detail';
export const RD_PRODUCT_POINT = 'commerce_rd_product_point';

/** Allow-listed warehouse-config read kinds resolved by the mounted owner command-plan route. */
export const RD_WAREHOUSE_PAGE_ROWS = 'commerce_rd_warehouse_page_rows';
export const RD_WAREHOUSE_MASTER_ROWS = 'commerce_rd_warehouse_master_rows';
export const RD_WAREHOUSE_ACTIVE = 'commerce_rd_warehouse_active';
export const RD_WAREHOUSES_ACTIVE = 'commerce_rd_warehouses_active';
export const RD_WAREHOUSE_BRANCH_ACTIVE = 'commerce_rd_warehouse_branch_active';

/**
 * Strict owner transport for the mounted native Commerce `command-plan` read
 * route. The axum owner plans the exact statement for a registered read kind;
 * this bridge emits no SQL, accepts no SQL from a caller and executes the
 * returned statement on the caller's already-open unit so the read observes
 * uncommitted effects in the caller's serializable transaction. Read kinds
 * carry no receipt/audit/outbox ledger. Missing configuration or any failed
 * configured request fails closed with no TypeScript fallback.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;
function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
  if (raw === undefined)
    throw new NativeCommerceReadError('COMMERCE_READ_UNAVAILABLE', 503, 'The native Commerce read service is not configured.');
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
    throw new NativeCommerceReadError('COMMERCE_READ_CONFIG_INVALID', 503, 'The native Commerce read transport is misconfigured.');
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeCommerceReadError('COMMERCE_READ_CONFIG_INVALID', 503, 'The native Commerce read transport is misconfigured.');
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    origin.protocol !== 'https:' ||
    typeof value.host !== 'string' ||
    value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000
  )
    throw new NativeCommerceReadError('COMMERCE_READ_CONFIG_INVALID', 503, 'The native Commerce read transport is misconfigured.');
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeCommerceReadError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommerceReadError';
    this.code = code;
    this.status = status;
  }
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === 'object' && !Array.isArray(v);

/** POST with an explicit Host header; fetch forbids overriding Host. */
function post(
  url: URL,
  headers: Record<string, string>,
  body: string,
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
        method: 'POST',
        agent,
        headers: { ...headers, 'content-length': String(Buffer.byteLength(body)) },
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
    outgoing.end(body);
  });
}

type Planned = { sql: string; values: unknown[] };

/** Ask the owner to plan one read; only the typed kind and positional values
 * cross the wire. The reply is validated to carry exactly one owner statement. */
async function plan(kind: string, values: readonly unknown[]): Promise<Planned> {
  const cfg = config();
  let response: { status: number; value: unknown };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/command-plan`, cfg.base),
      {
        host: cfg.host,
        origin: cfg.origin,
        accept: 'application/json',
        'content-type': 'application/json',
      },
      JSON.stringify({ kind, values }),
      cfg.timeoutMs,
    );
  } catch {
    throw new NativeCommerceReadError('COMMERCE_READ_UNAVAILABLE', 503, 'The native Commerce read service is unavailable.');
  }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommerceReadError(
      typeof error.code === 'string' ? error.code : 'COMMERCE_READ_FAILED',
      response.status,
      typeof error.message === 'string' ? error.message : 'The owned read could not be planned.',
    );
  }
  const statements = isRecord(value) && isRecord(value.data) ? value.data.statements : undefined;
  if (!Array.isArray(statements) || statements.length !== 1)
    throw new NativeCommerceReadError('COMMERCE_READ_RESPONSE_INVALID', 503, 'The native Commerce plan reply was invalid.');
  const entry = statements[0];
  if (
    !isRecord(entry) ||
    typeof entry.sql !== 'string' ||
    !Array.isArray(entry.values) ||
    entry.values.length > 64
  )
    throw new NativeCommerceReadError('COMMERCE_READ_RESPONSE_INVALID', 503, 'The native Commerce plan reply was invalid.');
  return { sql: entry.sql, values: entry.values as unknown[] };
}

/** Planned owner read returning all rows, executed on the caller's unit. */
export async function nativeCommerceReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T[]> {
  const statement = await plan(kind, values);
  return (await db.prepare(statement.sql).bind(...statement.values).all<T>()).results;
}

/** Planned owner read returning a single row (retired `.first()` parity). */
export async function nativeCommerceReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const statement = await plan(kind, values);
  return db.prepare(statement.sql).bind(...statement.values).first<T>();
}
