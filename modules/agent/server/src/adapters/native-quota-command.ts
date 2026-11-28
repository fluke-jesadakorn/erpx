import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';

/**
 * Strict server-side client for the native Agent AI-quota axum command route.
 * This is the only write path for the `ai_gateway_requests` reservation and
 * completion ledger. It carries no SQL and offers no TypeScript fallback.
 *
 * The AI transport is an internal caller without an HTTP Request object, so
 * the private loopback request is bound to a configured registered Holding:
 * Host, Origin and the Holding identity all come from validated configuration,
 * never from caller-supplied strings. The composition-owned route resolves the
 * Holding database and scope from that exact target before it runs the owner
 * unit. A missing transport or an invalid reply fails closed.
 */
export type NativeQuotaConfig = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativeQuotaScope = Readonly<{
  holdingId: string;
  companyId: string;
  branchId: string | null;
  connectionId: string | null;
  fundingSource: 'erpx' | 'byok';
  feature: string;
  requestId: string;
  usageId?: string;
}>;

export type NativeQuotaReserve = Readonly<{
  provider: string;
  model: string;
  now: number;
  windowStart: number;
  windowEnd: number;
  monthlyRequestLimit: number;
  reservationSatang: number;
}>;

export type NativeQuotaFinish = Readonly<{
  outcome: 'succeeded' | 'failed';
  inputTokens: number;
  outputTokens: number;
  units: Readonly<Record<string, number>>;
  now: number;
}>;

export class NativeQuotaError extends Error {
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native AI quota command could not be completed.');
    this.name = 'NativeQuotaError';
    this.status = status;
  }
}

const fail = (status?: number): never => {
  throw new NativeQuotaError('NATIVE_AGENT_QUOTA_UNAVAILABLE', status);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const identifier = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 200;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const safeInt = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;

function configuration(raw: string | undefined): NativeQuotaConfig {
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings))
    return fail();
  const timeoutMs = value.timeoutMs;
  if (typeof timeoutMs !== 'number' || !Number.isSafeInteger(timeoutMs) || timeoutMs < 250 || timeoutMs > 30000)
    return fail();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail();
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reservedPorts.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin
  )
    return fail();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' || typeof item.holdingId !== 'string')
      return fail();
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return fail();
    }
    if (
      origin.protocol !== 'https:' ||
      item.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !identifier(item.holdingId) ||
      !/^[a-z0-9_-]+$/.test(item.holdingSlug)
    )
      return fail();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  if (holdings.length < 1 || holdings.length > 64) return fail();
  return { privateBaseUrl: value.privateBaseUrl, holdings, timeoutMs };
}

function directHttp(
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = httpRequest(
      url,
      { method: 'POST', agent, headers: { ...headers, 'content-length': String(new TextEncoder().encode(body).length) }, signal, maxHeaderSize: 8192 },
      incoming => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          if (status !== 200) {
            incoming.destroy();
            resolve(new Response(null, { status: status >= 200 && status <= 599 ? status : 502 }));
            return;
          }
          const values = new Headers();
          for (let i = 0; i < incoming.rawHeaders.length; i += 2) values.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
          resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, { status, headers: values }));
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', error => { agent.destroy(); reject(error); });
    outgoing.write(body);
    outgoing.end();
  });
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > 16 * 1024) { await reader.cancel(); return fail(); }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) { data.set(chunk, offset); offset += chunk.byteLength; }
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return fail();
  }
}

async function dispatch(
  scope: NativeQuotaScope,
  body: Record<string, unknown>,
  key: string,
): Promise<Record<string, unknown>> {
  const config = configuration(process.env.ERPX_NATIVE_AGENT_QUOTA_CONFIG);
  const holding = config.holdings.find(item => item.holdingId === scope.holdingId);
  if (!holding || !/^[A-Za-z0-9:_.-]{8,180}$/.test(key)) return fail();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/agent/ai/quota`,
    config.privateBaseUrl,
  );
  const text = JSON.stringify(body);
  if (new TextEncoder().encode(text).length > 16 * 1024) return fail();
  const headers: Record<string, string> = {
    host: new URL(holding.origin).host,
    origin: holding.origin,
    'idempotency-key': key,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
  };
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await directHttp(url, headers, text, controller.signal);
    const payload = await boundedJson(response);
    if (!record(payload) || !record(payload.data)) return fail();
    return payload.data;
  } catch (error) {
    if (error instanceof NativeQuotaError) throw error;
    return fail();
  } finally {
    clearTimeout(timer);
  }
}

function scopeFacts(scope: NativeQuotaScope) {
  if (
    !identifier(scope.holdingId) || !identifier(scope.companyId) ||
    !identifier(scope.feature) || !identifier(scope.requestId) ||
    (scope.usageId !== undefined && !identifier(scope.usageId)) ||
    (scope.branchId !== null && (typeof scope.branchId !== 'string' || !identifier(scope.branchId))) ||
    (scope.connectionId !== null && (typeof scope.connectionId !== 'string' || !identifier(scope.connectionId))) ||
    (scope.fundingSource !== 'erpx' && scope.fundingSource !== 'byok')
  )
    return fail();
  return {
    requestId: scope.requestId,
    usageId: scope.usageId ?? scope.requestId,
    holdingId: scope.holdingId,
    companyId: scope.companyId,
    branchId: scope.branchId,
    connectionId: scope.connectionId,
    fundingSource: scope.fundingSource,
    feature: scope.feature,
  };
}

export function nativeAgentQuotaAvailable(): boolean {
  return process.env.ERPX_NATIVE_AGENT_QUOTA_CONFIG !== undefined;
}

/** Reserve one AI request. The native owner validates scope and inserts the
 * single `ai_gateway_requests` ledger row in the caller-bound Holding unit. */
export async function nativeReserveQuota(
  scope: NativeQuotaScope,
  reserve: NativeQuotaReserve,
  key: string,
): Promise<void> {
  if (
    !identifier(reserve.provider) || !identifier(reserve.model) ||
    !safeInt(reserve.now) || !safeInt(reserve.windowStart) || !safeInt(reserve.windowEnd) ||
    !safeInt(reserve.monthlyRequestLimit) || !safeInt(reserve.reservationSatang)
  )
    return fail();
  const data = await dispatch(scope, {
    kind: 'reserve',
    ...scopeFacts(scope),
    provider: reserve.provider,
    model: reserve.model,
    now: reserve.now,
    windowStart: reserve.windowStart,
    windowEnd: reserve.windowEnd,
    monthlyRequestLimit: reserve.monthlyRequestLimit,
    reservationSatang: reserve.reservationSatang,
  }, key);
  if (data.status !== 'reserved') return fail();
}

/** Complete one AI request exactly once. No TypeScript fallback. */
export async function nativeFinishQuota(
  scope: NativeQuotaScope,
  finish: NativeQuotaFinish,
  key: string,
): Promise<void> {
  if (
    (finish.outcome !== 'succeeded' && finish.outcome !== 'failed') ||
    !safeInt(finish.inputTokens) || !safeInt(finish.outputTokens) || !safeInt(finish.now) ||
    !record(finish.units) ||
    Object.entries(finish.units).some(([name, value]) => !identifier(name) || !safeInt(value))
  )
    return fail();
  const data = await dispatch(scope, {
    kind: 'finish',
    ...scopeFacts(scope),
    outcome: finish.outcome,
    inputTokens: finish.inputTokens,
    outputTokens: finish.outputTokens,
    units: finish.units,
    now: finish.now,
  }, key);
  if (data.status !== finish.outcome) return fail();
}
