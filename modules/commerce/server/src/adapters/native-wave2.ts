import 'server-only';
import { Agent, request as httpRequest } from 'node:http';

/**
 * Native Commerce wave2 warehouse-master transport. A thin private fetch client
 * that forwards the exact registered Holding Host/Origin, one deterministic
 * Idempotency-Key and the caller identity to the Commerce axum command route
 * and validates the exact acknowledgement. No SQL, no fallback writer: missing
 * or malformed configuration fails closed.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_COMMERCE_WAVE2_CONFIG;
  if (raw === undefined)
    throw new NativeCommerceWave2Error(
      'COMMERCE_WAVE2_UNAVAILABLE',
      503,
      'The native Commerce warehouse service is not configured.',
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
    throw new NativeCommerceWave2Error(
      'COMMERCE_WAVE2_CONFIGURATION_INVALID',
      503,
      'The native Commerce warehouse transport is misconfigured.',
    );
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeCommerceWave2Error(
      'COMMERCE_WAVE2_CONFIGURATION_INVALID',
      503,
      'The native Commerce warehouse transport is misconfigured.',
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
    throw new NativeCommerceWave2Error(
      'COMMERCE_WAVE2_CONFIGURATION_INVALID',
      503,
      'The native Commerce warehouse transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeCommerceWave2Error extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommerceWave2Error';
    this.code = code;
    this.status = status;
  }
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export type NativeWarehouseSaveFields = Readonly<{
  id: string | null;
  record: string;
  companyId: string;
  branchId: string;
  code: string;
  name: string;
  status: string;
  now: number;
  actorId: string;
}>;


/** Post with an explicit Host header; fetch forbids overriding Host. */
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
          try { value = text ? JSON.parse(text) : null; } catch { value = null; }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', error => { agent.destroy(); reject(error); });
      },
    );
    outgoing.on('error', error => { agent.destroy(); reject(error); });
    outgoing.end(body);
  });
}

/** Commit one warehouse master write through the native axum command route. */
export async function nativeWarehouseSave(
  fields: NativeWarehouseSaveFields,
  idempotencyKey: string,
): Promise<{ id: string; companyId: string }> {
  const cfg = config();
  let reply: { status: number; value: unknown };
  try {
    reply = await post(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/warehouse-save`,
        cfg.base,
      ),
      {
        host: cfg.host,
        origin: cfg.origin,
        accept: 'application/json',
        'content-type': 'application/json',
        'idempotency-key': idempotencyKey,
        'x-erpx-context-user': fields.actorId,
      },
      JSON.stringify(fields),
      cfg.timeoutMs,
    );
  } catch {
    throw new NativeCommerceWave2Error(
      'COMMERCE_WAVE2_UNAVAILABLE',
      503,
      'The native Commerce warehouse service is unavailable.',
    );
  }
  const value = reply.value;
  if (reply.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommerceWave2Error(
      typeof error.code === 'string' ? error.code : 'COMMERCE_WAREHOUSE_SAVE_FAILED',
      reply.status,
      typeof error.message === 'string' ? error.message : 'The warehouse could not be saved.',
    );
  }
  // The warehouse route replies with the owner acknowledgement directly; some
  // earlier native routes wrapped it under data. Accept both exact shapes.
  const data = isRecord(value) && isRecord(value.data) ? value.data : value;
  if (
    !isRecord(data) ||
    typeof data.id !== 'string' ||
    typeof data.companyId !== 'string'
  )
    throw new NativeCommerceWave2Error(
      'COMMERCE_WAVE2_RESPONSE_INVALID',
      503,
      'The native Commerce acknowledgement was invalid.',
    );
  return { id: data.id, companyId: data.companyId };
}
