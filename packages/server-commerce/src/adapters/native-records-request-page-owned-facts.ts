import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/**
 * Native Commerce read transport for the Records request-page currency fact.
 * This is the successor of the SQL reader in
 * `adapters/records-request-page-owned-facts.ts`: it carries no SQL text and no
 * fallback reader. A missing, malformed or failing native runtime fails closed,
 * so the owner crate stays the sole authority.
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
    throw new NativeCommerceCurrencyReadError(
      'COMMERCE_CURRENCY_READ_UNAVAILABLE',
      503,
      'The native Commerce currency service is not configured.',
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
    throw new NativeCommerceCurrencyReadError(
      'COMMERCE_CURRENCY_READ_CONFIG_INVALID',
      503,
      'The native Commerce currency transport is misconfigured.',
    );
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeCommerceCurrencyReadError(
      'COMMERCE_CURRENCY_READ_CONFIG_INVALID',
      503,
      'The native Commerce currency transport is misconfigured.',
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
    throw new NativeCommerceCurrencyReadError(
      'COMMERCE_CURRENCY_READ_CONFIG_INVALID',
      503,
      'The native Commerce currency transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeCommerceCurrencyReadError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommerceCurrencyReadError';
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

/**
 * Native-backed Commerce currency reader. `database` is retained in the
 * signature for call-site compatibility only; no SQL is issued on it. The
 * native owner executes the exact Company-scoped currency read.
 */
export function RequestPageCommerceOwnedFacts(_database: SqlDatabase) {
  return {
    async readRequestPageOptionsNodesResultcompanySettings2(companyId: string): Promise<{ currency: string } | null> {
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
          },
          cfg.timeoutMs,
        );
      } catch {
        throw new NativeCommerceCurrencyReadError(
          'COMMERCE_CURRENCY_READ_UNAVAILABLE',
          503,
          'The native Commerce currency service is unavailable.',
        );
      }
      const value: unknown = response.value;
      if (response.status !== 200) {
        const error = isRecord(value) && isRecord(value.error) ? value.error : {};
        throw new NativeCommerceCurrencyReadError(
          typeof error.code === 'string' ? error.code : 'COMMERCE_CURRENCY_READ_FAILED',
          response.status,
          typeof error.message === 'string' ? error.message : 'The Commerce currency could not be read.',
        );
      }
      if (!isRecord(value) || !('currency' in value))
        throw new NativeCommerceCurrencyReadError(
          'COMMERCE_CURRENCY_READ_RESPONSE_INVALID',
          503,
          'The native Commerce currency reply was invalid.',
        );
      const currency = value.currency;
      if (currency === null || currency === undefined) return null;
      if (typeof currency !== 'string')
        throw new NativeCommerceCurrencyReadError(
          'COMMERCE_CURRENCY_READ_RESPONSE_INVALID',
          503,
          'The native Commerce currency reply was invalid.',
        );
      return { currency };
    },
  };
}
