import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import type { SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';

/**
 * Thin fail-closed transport to the single native Manufacturing owner axum
 * router. `values` is a positional data array, never SQL text. The owner crate
 * selects the authoritative SQL and executes it in its own serializable unit.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

export type ManufacturingOwnerWriteKind =
  | 'production.posting'
  | 'production.cost-movement'
  | 'order.create'
  | 'order.release-event'
  | 'order.reservation'
  | 'order.output-report'
  | 'order.operation-event'
  | 'order.reservation-release'
  | 'order.transition'
  | 'order.event';

export class NativeManufacturingOwnerWriteError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeManufacturingOwnerWriteError';
    this.code = code;
    this.status = status;
  }
}

function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_MANUFACTURING_OWNER_WRITE_CONFIG;
  if (raw === undefined)
    throw new NativeManufacturingOwnerWriteError(
      'MANUFACTURING_OWNER_WRITE_UNAVAILABLE',
      503,
      'The native Manufacturing service is not configured.',
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
    throw new NativeManufacturingOwnerWriteError(
      'MANUFACTURING_OWNER_WRITE_CONFIG_INVALID',
      503,
      'The native Manufacturing transport is misconfigured.',
    );
  }
  let base: URL;
  let origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeManufacturingOwnerWriteError(
      'MANUFACTURING_OWNER_WRITE_CONFIG_INVALID',
      503,
      'The native Manufacturing transport is misconfigured.',
    );
  }
  const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    origin.protocol !== 'https:' ||
    typeof value.host !== 'string' ||
    value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30000
  )
    throw new NativeManufacturingOwnerWriteError(
      'MANUFACTURING_OWNER_WRITE_CONFIG_INVALID',
      503,
      'The native Manufacturing transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

/** Send one typed owner write to the native owner and return its acknowledgement. */
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

export async function nativeManufacturingOwnerWrite(
  kind: ManufacturingOwnerWriteKind,
  values: readonly unknown[],
  recordId: string,
  actorId: string,
  idempotencyKey: string,
  companyId: string,
): Promise<{ id: string; kind: string }> {
  const cfg = config();
  let response: { status: number; value: unknown };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/manufacturing/owner-write`, cfg.base),
      {
        host: cfg.host,
        origin: cfg.origin,
        accept: 'application/json',
        'content-type': 'application/json',
        'idempotency-key': idempotencyKey,
        'x-erpx-context-user': actorId,
      },
      JSON.stringify({ kind, values, recordId, companyId }),
      cfg.timeoutMs,
    );
  } catch {
    throw new NativeManufacturingOwnerWriteError(
      'MANUFACTURING_OWNER_WRITE_UNAVAILABLE',
      503,
      'The native Manufacturing service is unavailable.',
    );
  }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeManufacturingOwnerWriteError(
      typeof error.code === 'string' ? error.code : 'MANUFACTURING_OWNER_WRITE_FAILED',
      response.status,
      typeof error.message === 'string' ? error.message : 'The Manufacturing write could not be completed.',
    );
  }
  if (
    !isRecord(value) ||
    !isRecord(value.data) ||
    typeof value.data.id !== 'string' ||
    typeof value.data.kind !== 'string'
  )
    throw new NativeManufacturingOwnerWriteError(
      'MANUFACTURING_OWNER_WRITE_RESPONSE_INVALID',
      503,
      'The native Manufacturing acknowledgement was invalid.',
    );
  return { id: value.data.id, kind: value.data.kind };
}

/**
 * Synchronous marker returned by retired repository write methods. It carries
 * only typed data; no SQL is created or retained on the TypeScript side.
 */
export type NativeOwnerWriteMarker = Readonly<{
  __manufacturingOwnerWrite: true;
  kind: ManufacturingOwnerWriteKind;
  values: readonly unknown[];
  recordId: string;
}>;

export function ownerWrite(
  kind: ManufacturingOwnerWriteKind,
  values: readonly unknown[],
  recordId: string,
): SqlStatement {
  return {
    __manufacturingOwnerWrite: true,
    kind,
    values,
    recordId,
  } as unknown as SqlStatement;
}

export function isOwnerWrite(value: unknown): value is NativeOwnerWriteMarker {
  return isRecord(value) && value.__manufacturingOwnerWrite === true;
}

/**
 * Flush every native marker produced by one command before the retained
 * workflow receipt is committed. A missing native service fails closed.
 */
export async function flushOwnerWrites(
  effects: readonly unknown[],
  actor: Pick<WorkContext, 'userId' | 'companyId'>,
  commandKey: string,
): Promise<SqlStatement[]> {
  const markers = effects.filter(isOwnerWrite);
  for (const [index, marker] of markers.entries())
    await nativeManufacturingOwnerWrite(
      marker.kind,
      marker.values,
      marker.recordId,
      actor.userId,
      `${commandKey}:${index}:${marker.kind}`,
      actor.companyId,
    );
  return effects.filter(effect => !isOwnerWrite(effect)) as SqlStatement[];
}
