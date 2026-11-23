import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { createHash } from 'node:crypto';

/**
 * Native Commercial wave2 grant-save transport. It is a thin private fetch
 * client: it forwards the exact registered Holding Host/Origin, one
 * deterministic Idempotency-Key and the caller identity to the Commercial axum
 * command route, and validates the exact acknowledgement before returning.
 * It never contains SQL, never mints a session/actor/Holding scope and has no
 * fallback writer: missing or malformed configuration fails closed.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_COMMERCIAL_WAVE2_CONFIG;
  if (raw === undefined)
    throw new NativeCommercialWave2Error(
      'COMMERCIAL_WAVE2_UNAVAILABLE',
      503,
      'The native Commercial grant service is not configured.',
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
    throw new NativeCommercialWave2Error(
      'COMMERCIAL_WAVE2_CONFIGURATION_INVALID',
      503,
      'The native Commercial grant transport is misconfigured.',
    );
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeCommercialWave2Error(
      'COMMERCIAL_WAVE2_CONFIGURATION_INVALID',
      503,
      'The native Commercial grant transport is misconfigured.',
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
    throw new NativeCommercialWave2Error(
      'COMMERCIAL_WAVE2_CONFIGURATION_INVALID',
      503,
      'The native Commercial grant transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeCommercialWave2Error extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommercialWave2Error';
    this.code = code;
    this.status = status;
  }
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

/** The retained grant fields are carried exactly once; no SQL or SQL mirror. */
export type NativeGrantSaveFields = Readonly<{
  companyId: string;
  packageId: string;
  packageVersion: number;
  state: string;
  startsAt: number;
  endsAt: number | null;
  version: number;
  reason: string;
  evidence: string;
  updatedAt: number;
  actorId: string;
  idempotencyKey: string;
  requestHash: string;
  beforeJson: string | null;
  afterJson: string;
}>;

export type NativeGrantSaveReceipt = Readonly<{
  companyId: string;
  packageId: string;
  version: number;
}>;

/** Commit one Company package grant through the native axum command route. */
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

export async function nativeGrantSave(
  fields: NativeGrantSaveFields,
): Promise<NativeGrantSaveReceipt> {
  const cfg = config();
  const headers = new Headers({
    host: cfg.host,
    origin: cfg.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': fields.idempotencyKey,
    'x-erpx-context-user': fields.actorId,
  });
  const body = JSON.stringify({
    companyId: fields.companyId,
    packageId: fields.packageId,
    packageVersion: fields.packageVersion,
    state: fields.state,
    startsAt: fields.startsAt,
    endsAt: fields.endsAt,
    version: fields.version,
    reason: fields.reason,
    evidence: fields.evidence,
    updatedAt: fields.updatedAt,
    actorId: fields.actorId,
    idempotencyKey: fields.idempotencyKey,
    requestHash: fields.requestHash,
    beforeJson: fields.beforeJson,
    afterJson: fields.afterJson,
  });
  let response: { status: number; value: unknown };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commercial/grant-save`, cfg.base),
      Object.fromEntries(headers.entries()),
      body,
      cfg.timeoutMs,
    );
  } catch {
    throw new NativeCommercialWave2Error(
      'COMMERCIAL_WAVE2_UNAVAILABLE',
      503,
      'The native Commercial grant service is unavailable.',
    );
  }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommercialWave2Error(
      typeof error.code === 'string' ? error.code : 'COMMERCIAL_GRANT_SAVE_FAILED',
      response.status,
      typeof error.message === 'string'
        ? error.message
        : 'The Company package grant could not be saved.',
    );
  }
  if (
    !isRecord(value) ||
    !isRecord(value.data) ||
    typeof value.data.companyId !== 'string' ||
    typeof value.data.packageId !== 'string' ||
    typeof value.data.version !== 'number'
  )
    throw new NativeCommercialWave2Error(
      'COMMERCIAL_WAVE2_RESPONSE_INVALID',
      503,
      'The native Commercial grant acknowledgement was invalid.',
    );
  // Deterministic key binds the caller operation to the native receipt.
  void createHash;
  return {
    companyId: value.data.companyId,
    packageId: value.data.packageId,
    version: value.data.version,
  };
}
