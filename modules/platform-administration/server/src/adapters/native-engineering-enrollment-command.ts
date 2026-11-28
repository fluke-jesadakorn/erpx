import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';

/**
 * Strict server-side client for the native engineering enrollment axum route.
 * The attestation/database authority checks stay in the retained application;
 * this client only forwards the already-validated write facts. No SQL and no
 * fallback: a missing transport or invalid reply fails closed.
 */
export type NativeEngineeringEnrollmentConfig = Readonly<{
  privateBaseUrl: string;
  holdingSlug: string;
  holdingId: string;
  origin: string;
  timeoutMs: number;
}>;

export type NativeEngineeringEnrollmentInput = Readonly<{
  run: string;
  sourceFingerprint: string;
  softwareVersion: string;
  fileManifestSha256: string;
  companyIds: readonly string[];
  disabledCompanyIds: readonly string[];
  expiresAt?: number;
  now: number;
}>;

export class NativeEngineeringEnrollmentError extends Error {
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native engineering enrollment command could not be completed.');
    this.name = 'NativeEngineeringEnrollmentError';
    this.status = status;
  }
}

const fail = (status?: number): never => {
  throw new NativeEngineeringEnrollmentError('NATIVE_PLATFORM_UNAVAILABLE', status);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: string | undefined): NativeEngineeringEnrollmentConfig {
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string') return fail();
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
    base.username || base.password || value.privateBaseUrl !== base.origin
  )
    return fail();
  if (
    typeof value.holdingSlug !== 'string' || !/^[a-z0-9_-]+$/.test(value.holdingSlug) ||
    typeof value.holdingId !== 'string' || value.holdingId.length === 0 ||
    typeof value.origin !== 'string'
  )
    return fail();
  const origin = value.origin;
  if (!(
    origin === 'http://127.0.0.1:3104' || origin === 'http://localhost:3104' ||
    (origin.startsWith('https://') && origin === new URL(origin).origin)
  ))
    return fail();
  return {
    privateBaseUrl: value.privateBaseUrl,
    holdingSlug: value.holdingSlug,
    holdingId: value.holdingId,
    origin,
    timeoutMs,
  };
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

export function nativeEngineeringEnrollmentAvailable(): boolean {
  return process.env.ERPX_NATIVE_PLATFORM_ENGINEERING_CONFIG !== undefined;
}

/** Execute the already-validated engineering enrollment through the native
 * axum command. The write is owned in Rust; no TypeScript SQL remains. */
export async function nativeEnrollEngineeringVerification(
  input: NativeEngineeringEnrollmentInput,
  key: string,
): Promise<Record<string, unknown>> {
  const config = configuration(process.env.ERPX_NATIVE_PLATFORM_ENGINEERING_CONFIG);
  if (!/^[A-Za-z0-9:_.-]{8,180}$/.test(key)) return fail();
  if (!/^[a-f0-9]{12}$/.test(input.run) || !/^[a-f0-9]{64}$/.test(input.sourceFingerprint) ||
      !/^[a-f0-9]{64}$/.test(input.fileManifestSha256) ||
      input.softwareVersion !== `release-${input.sourceFingerprint}` ||
      input.companyIds.length + input.disabledCompanyIds.length < 1 ||
      input.companyIds.length > 100 || input.disabledCompanyIds.length > 100 ||
      !Number.isSafeInteger(input.now) ||
      (input.expiresAt !== undefined && !Number.isSafeInteger(input.expiresAt)))
    return fail();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/platform/engineering-verification/enroll`,
    config.privateBaseUrl,
  );
  const body = JSON.stringify(input);
  if (new TextEncoder().encode(body).length > 64 * 1024) return fail();
  const headers: Record<string, string> = {
    host: new URL(config.origin).host,
    origin: config.origin,
    'idempotency-key': key,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
  };
  headers['x-erpx-context-company'] = config.holdingId;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await directHttp(url, headers, body, controller.signal);
    if (response.redirected || response.status !== 200 || !response.body) return fail(response.status);
    const text = await response.text();
    if (text.length > 64 * 1024) return fail();
    const payload = JSON.parse(text) as { data?: unknown };
    if (!record(payload) || !record(payload.data) || payload.data.scope !== 'isolated-engineering') return fail();
    return payload.data;
  } catch (error) {
    if (error instanceof NativeEngineeringEnrollmentError) throw error;
    return fail();
  } finally {
    clearTimeout(timer);
  }
}
