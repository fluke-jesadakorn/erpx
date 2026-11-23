import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Native Commerce read transport for a Quality receiving-origin fact. This is
 * the owner-local successor of the receipt half of
 * `adapters/quality-purchase-sources.ts`: it carries no SQL text and no fallback
 * reader. A missing, malformed or failing native runtime fails closed, so the
 * owner crate stays the sole authority. The read route is mounted on the same
 * axum service as the other Commerce owner routes.
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
    throw new NativeCommerceQualityReceiptOriginError(
      'QUALITY_RECEIPT_ORIGIN_READ_UNAVAILABLE',
      503,
      'The native Quality receipt-origin service is not configured.',
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
    throw new NativeCommerceQualityReceiptOriginError(
      'QUALITY_RECEIPT_ORIGIN_READ_CONFIG_INVALID',
      503,
      'The native Quality receipt-origin transport is misconfigured.',
    );
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeCommerceQualityReceiptOriginError(
      'QUALITY_RECEIPT_ORIGIN_READ_CONFIG_INVALID',
      503,
      'The native Quality receipt-origin transport is misconfigured.',
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
    throw new NativeCommerceQualityReceiptOriginError(
      'QUALITY_RECEIPT_ORIGIN_READ_CONFIG_INVALID',
      503,
      'The native Quality receipt-origin transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeCommerceQualityReceiptOriginError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommerceQualityReceiptOriginError';
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

export type QualityReceiptOrigin = Readonly<{
  orderId: string;
  companyId: string;
  branchId: string;
  warehouseId: string;
  productId: string;
  fulfillmentLineId: string;
}>;

const isOrigin = (value: unknown): value is QualityReceiptOrigin =>
  isRecord(value) &&
  typeof value.orderId === 'string' &&
  typeof value.companyId === 'string' &&
  typeof value.branchId === 'string' &&
  typeof value.warehouseId === 'string' &&
  typeof value.productId === 'string' &&
  typeof value.fulfillmentLineId === 'string';

/**
 * Native-backed receiving-origin reader. `database` is retained in the
 * signature for call-site compatibility only; no SQL is issued on it. The
 * native owner executes the exact Company/branch-scoped fact lookup.
 */
export async function qualityReceiptOriginFacts(
  _database: SqlDatabase,
  actor: Pick<WorkContext, 'companyId'>,
  fulfillmentLineId: string,
  branchId?: string,
): Promise<QualityReceiptOrigin | null> {
  const cfg = config();
  let response: { status: number; value: unknown };
  try {
    response = await get(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/quality-receipt-origin`,
        cfg.base,
      ),
      {
        host: cfg.host,
        origin: cfg.origin,
        accept: 'application/json',
        'x-erpx-context-company': actor.companyId,
        ...(branchId === undefined ? {} : { 'x-erpx-context-branch': branchId }),
        'x-erpx-context-fulfillment-line': fulfillmentLineId,
      },
      cfg.timeoutMs,
    );
  } catch {
    throw new NativeCommerceQualityReceiptOriginError(
      'QUALITY_RECEIPT_ORIGIN_READ_UNAVAILABLE',
      503,
      'The native Quality receipt-origin service is unavailable.',
    );
  }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommerceQualityReceiptOriginError(
      typeof error.code === 'string' ? error.code : 'QUALITY_RECEIPT_ORIGIN_READ_FAILED',
      response.status,
      typeof error.message === 'string'
        ? error.message
        : 'The Quality receiving origin could not be read.',
    );
  }
  if (!isRecord(value) || !('origin' in value))
    throw new NativeCommerceQualityReceiptOriginError(
      'QUALITY_RECEIPT_ORIGIN_READ_RESPONSE_INVALID',
      503,
      'The native Quality receipt-origin reply was invalid.',
    );
  const origin = value.origin;
  if (origin === null || origin === undefined) return null;
  if (!isOrigin(origin))
    throw new NativeCommerceQualityReceiptOriginError(
      'QUALITY_RECEIPT_ORIGIN_READ_RESPONSE_INVALID',
      503,
      'The native Quality receipt-origin reply was invalid.',
    );
  return origin;
}
