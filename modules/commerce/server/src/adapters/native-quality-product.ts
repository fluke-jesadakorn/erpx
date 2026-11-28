import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';

/**
 * Native Commerce read transport for the Quality active-product guard. This is
 * the successor of the SQL reader in `adapters/quality-product.ts`: it carries
 * no SQL text and no fallback reader. A missing, malformed or failing native
 * runtime fails closed, so the owner crate stays the sole authority.
 *
 * Reuses the already-registered native Commerce owner transport; the read
 * route is mounted on the same axum service as the owner command routes.
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
    throw new NativeCommerceQualityProductError(
      'QUALITY_PRODUCT_READ_UNAVAILABLE',
      503,
      'The native Quality product service is not configured.',
    );
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
    throw new NativeCommerceQualityProductError(
      'QUALITY_PRODUCT_READ_CONFIG_INVALID',
      503,
      'The native Quality product transport is misconfigured.',
    );
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeCommerceQualityProductError(
      'QUALITY_PRODUCT_READ_CONFIG_INVALID',
      503,
      'The native Quality product transport is misconfigured.',
    );
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
    throw new NativeCommerceQualityProductError(
      'QUALITY_PRODUCT_READ_CONFIG_INVALID',
      503,
      'The native Quality product transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeCommerceQualityProductError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommerceQualityProductError';
    this.code = code;
    this.status = status;
  }
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === 'object' && !Array.isArray(v);

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

type Row = Record<string, string | number | null>;

/**
 * Native-backed Quality active-product reader. `database` is retained in the
 * signature for call-site compatibility only; no SQL is issued on it. The
 * native owner executes the exact Company-scoped active-product read.
 */
export async function qualityProductFacts(
  _database: SqlDatabase,
  actor: Pick<WorkContext, 'companyId'>,
  id: string,
): Promise<Row | null> {
  const cfg = config();
  let response: { status: number; value: unknown };
  try {
    response = await get(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/quality-product`,
        cfg.base,
      ),
      {
        host: cfg.host,
        origin: cfg.origin,
        accept: 'application/json',
        'x-erpx-context-company': actor.companyId,
        'x-erpx-context-product': id,
      },
      cfg.timeoutMs,
    );
  } catch {
    throw new NativeCommerceQualityProductError(
      'QUALITY_PRODUCT_READ_UNAVAILABLE',
      503,
      'The native Quality product service is unavailable.',
    );
  }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommerceQualityProductError(
      typeof error.code === 'string' ? error.code : 'QUALITY_PRODUCT_READ_FAILED',
      response.status,
      typeof error.message === 'string' ? error.message : 'The Quality product could not be read.',
    );
  }
  if (!isRecord(value) || !('product' in value))
    throw new NativeCommerceQualityProductError(
      'QUALITY_PRODUCT_READ_RESPONSE_INVALID',
      503,
      'The native Quality product reply was invalid.',
    );
  const product = value.product;
  if (product === null || product === undefined) return null;
  if (
    !isRecord(product) ||
    typeof product.id !== 'string' ||
    !Number.isSafeInteger(product.version)
  )
    throw new NativeCommerceQualityProductError(
      'QUALITY_PRODUCT_READ_RESPONSE_INVALID',
      503,
      'The native Quality product reply was invalid.',
    );
  return { id: product.id, version: product.version as number };
}
