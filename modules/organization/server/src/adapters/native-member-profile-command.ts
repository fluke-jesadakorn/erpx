import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';

/**
 * Strict server-side client for the native member-profile axum command route.
 * This is the only write path for `member.profile.update`: it carries no SQL
 * and offers no TypeScript fallback. The composition-owned route resolves the
 * Account session, current IAM, identity membership, serializable owner unit
 * and the receipt/audit/outbox ledger.
 */
export type NativeMemberProfileConfig = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativeMemberProfileInput = Readonly<{
  bio: string;
  links: readonly Readonly<{ label: string; url: string }>[];
  version: number;
}>;

export type NativeMemberProfileResult = Readonly<{
  version: number;
  bio: string;
  links: { label: string; url: string }[];
}>;

export class NativeMemberProfileError extends Error {
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native member profile command could not be completed.');
    this.name = 'NativeMemberProfileError';
    this.status = status;
  }
}

const fail = (status?: number): never => {
  throw new NativeMemberProfileError('NATIVE_MEMBER_PROFILE_UNAVAILABLE', status);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const identifier = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 200;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: string | undefined): NativeMemberProfileConfig {
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

function nativeConfigured(): boolean {
  return process.env.ERPX_NATIVE_ORGANIZATION_CONFIG !== undefined;
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
      if (bytes > 64 * 1024) { await reader.cancel(); return fail(); }
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

function result(value: unknown): NativeMemberProfileResult {
  if (!record(value) || !record(value.data)) return fail();
  const data = value.data;
  const links = data.links;
  if (
    typeof data.version !== 'number' || !Number.isSafeInteger(data.version) || data.version < 1 ||
    typeof data.bio !== 'string' || !Array.isArray(links) || links.length > 3 ||
    !links.every(link => record(link) && typeof link.label === 'string' && typeof link.url === 'string')
  )
    return fail();
  return { version: data.version, bio: data.bio, links: links as { label: string; url: string }[] };
}

/** Whether the native command transport is configured for this process. */
export function nativeMemberProfileAvailable(): boolean {
  return nativeConfigured();
}

/** Save the caller's own profile through the native axum command route. No SQL
 * and no fallback: a missing transport or an invalid reply fails closed. */
export async function nativeSaveMemberProfile(
  actor: WorkContext,
  input: NativeMemberProfileInput,
  key: string,
): Promise<NativeMemberProfileResult> {
  const config = configuration(process.env.ERPX_NATIVE_ORGANIZATION_CONFIG);
  const holding = config.holdings.find(item => item.holdingSlug === actor.holdingSlug && item.holdingId === actor.holdingId);
  if (!holding || actor.authority === undefined || !/^[A-Za-z0-9:_.-]{8,180}$/.test(key))
    return fail();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/people/profiles/me`,
    config.privateBaseUrl,
  );
  const body = JSON.stringify({ bio: input.bio, links: input.links, version: input.version });
  if (new TextEncoder().encode(body).length > 64 * 1024) return fail();
  const headers: Record<string, string> = {
    host: new URL(holding.origin).host,
    origin: holding.origin,
    'idempotency-key': key,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
  };
  // The route is a private loopback ingress. The Next route has already
  // resolved the Account session into `actor`; forward its bounded current
  // context exactly as the organization workspace client does, never a minted
  // identity. Composition resolves and re-validates it before the owner unit.
  if (actor.authority) {
    headers['x-erpx-context-company'] = actor.companyId;
    headers['x-erpx-context-assignment'] = actor.assignmentId;
    headers['x-erpx-context-revision'] = String(actor.authority.revision);
    headers['x-erpx-context-assignment-version'] = String(actor.authority.assignmentVersion);
  }
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 30000);
  try {
    const response = await directHttp(url, headers, body, controller.signal);
    return result(await boundedJson(response));
  } catch (error) {
    if (error instanceof NativeMemberProfileError) throw error;
    return fail();
  } finally {
    clearTimeout(timer);
  }
}
