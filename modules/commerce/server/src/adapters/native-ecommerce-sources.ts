import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

/**
 * Native Commerce read transport for the Ecommerce functional-currency
 * configuration. This is the successor of the SQL reader in
 * `adapters/ecommerce-sources.ts`: it carries no SQL text and no fallback
 * reader. A missing, malformed or failing native runtime fails closed, so the
 * owner crate stays the sole authority.
 *
 * It reuses the already-mounted native Commerce currency route on the same axum
 * service as the owner command routes. The owner returns the Company-scoped
 * configuration when the opt-in header is present.
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
    throw new NativeCommerceEcommerceSourcesError(
      'ECOMMERCE_CONFIGURATION_READ_UNAVAILABLE',
      503,
      'The native Commerce configuration service is not configured.',
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
    throw new NativeCommerceEcommerceSourcesError(
      'ECOMMERCE_CONFIGURATION_READ_CONFIG_INVALID',
      503,
      'The native Commerce configuration transport is misconfigured.',
    );
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeCommerceEcommerceSourcesError(
      'ECOMMERCE_CONFIGURATION_READ_CONFIG_INVALID',
      503,
      'The native Commerce configuration transport is misconfigured.',
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
    throw new NativeCommerceEcommerceSourcesError(
      'ECOMMERCE_CONFIGURATION_READ_CONFIG_INVALID',
      503,
      'The native Commerce configuration transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeCommerceEcommerceSourcesError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommerceEcommerceSourcesError';
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

const currencyCode = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Z]{3}$/.test(value);

/**
 * Native-backed Ecommerce functional-currency configuration reader. `database`
 * is retained in the signature for call-site compatibility only; no SQL is
 * issued on it. The native owner executes the exact Company-scoped read.
 */
export async function ecommerceCurrencyConfiguration(
  _database: SqlDatabase,
  companyId: string,
): Promise<{ currency: string; base_currency: string } | null> {
  const cfg = config();
  let response: { status: number; value: unknown };
  try {
    response = await get(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/request-page-currency`,
        cfg.base,
      ),
      {
        host: cfg.host,
        origin: cfg.origin,
        accept: 'application/json',
        'x-erpx-context-company': companyId,
        'x-erpx-ecommerce-configuration': '1',
      },
      cfg.timeoutMs,
    );
  } catch {
    throw new NativeCommerceEcommerceSourcesError(
      'ECOMMERCE_CONFIGURATION_READ_UNAVAILABLE',
      503,
      'The native Commerce configuration service is unavailable.',
    );
  }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommerceEcommerceSourcesError(
      typeof error.code === 'string' ? error.code : 'ECOMMERCE_CONFIGURATION_READ_FAILED',
      response.status,
      typeof error.message === 'string' ? error.message : 'The Commerce configuration could not be read.',
    );
  }
  if (!isRecord(value) || !('configuration' in value))
    throw new NativeCommerceEcommerceSourcesError(
      'ECOMMERCE_CONFIGURATION_READ_RESPONSE_INVALID',
      503,
      'The native Commerce configuration reply was invalid.',
    );
  const configuration = value.configuration;
  if (configuration === null || configuration === undefined) return null;
  if (
    !isRecord(configuration) ||
    !currencyCode(configuration.currency) ||
    !currencyCode(configuration.base_currency)
  )
    throw new NativeCommerceEcommerceSourcesError(
      'ECOMMERCE_CONFIGURATION_READ_RESPONSE_INVALID',
      503,
      'The native Commerce configuration reply was invalid.',
    );
  return {
    currency: configuration.currency,
    base_currency: configuration.base_currency,
  };
}
