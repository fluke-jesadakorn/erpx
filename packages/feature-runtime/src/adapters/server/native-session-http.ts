import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type { AccountSessionResponse } from '@erpx/contracts/account-auth';

/** Explicit private transport and HTTPS public authority are separate contexts. */
export type NativeSessionClientConfiguration = Readonly<{
  privateBaseUrl: string;
  accountOrigin: string;
  holdingOrigins: readonly string[];
  timeoutMs: number;
}>;
export class NativeSessionReadError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native session query could not be completed.');
    this.name = 'NativeSessionReadError';
    this.code = code;
    this.status = status;
  }
}
const fail = (code: string, status?: number): never => { throw new NativeSessionReadError(code, status); };
const protectedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
function exactOrigin(value: string): URL {
  let url: URL;
  try { url = new URL(value); } catch { return fail('NATIVE_SESSION_CONFIGURATION_INVALID'); }
  if (url.protocol !== 'https:' || url.username || url.password || url.port || url.pathname !== '/' || url.search || url.hash
    || url.hostname.length > 253 || !url.hostname.includes('.') || !url.hostname.split('.').every(part => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(part)))
    return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  if (value !== url.origin) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  return url;
}
function endpoint(value: string): URL {
  let url: URL;
  try { url = new URL(value); } catch { return fail('NATIVE_SESSION_CONFIGURATION_INVALID'); }
  const port = Number(url.port);
  if (url.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(url.hostname) || !url.port || !Number.isInteger(port)
    || port < 1 || port > 65535 || protectedPorts.has(port) || port >= 3200 && port <= 3299
    || url.username || url.password || url.pathname !== '/' || url.search || url.hash)
    return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  if (value !== url.origin) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  return new URL('/api/auth/get-session', url);
}
function object(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)
    || Object.keys(value).length !== keys.length || !keys.every(key => Object.hasOwn(value, key))) return fail('NATIVE_SESSION_RESPONSE_INVALID');
  return value as Record<string, unknown>;
}
function text(value: unknown, maximum: number): string {
  if (typeof value !== 'string' || new TextEncoder().encode(value).length > maximum) return fail('NATIVE_SESSION_RESPONSE_INVALID');
  return value;
}
function nonempty(value: unknown, maximum: number): string {
  const result = text(value, maximum);
  return result || fail('NATIVE_SESSION_RESPONSE_INVALID');
}
function date(value: unknown): string {
  const valueText = text(value, 64);
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/.test(valueText) || !Number.isFinite(Date.parse(valueText)))
    return fail('NATIVE_SESSION_RESPONSE_INVALID');
  return valueText;
}
function session(value: unknown): AccountSessionResponse | null {
  if (value === null) return null;
  const root = object(value, ['session', 'user']);
  const current = object(root.session, ['id', 'userId', 'expiresAt', 'createdAt', 'updatedAt']);
  const user = object(root.user, ['id', 'name', 'email', 'emailVerified', 'image']);
  const userId = text(user.id, 200), sessionUser = text(current.userId, 200);
  if (!userId || userId !== sessionUser || typeof user.emailVerified !== 'boolean') return fail('NATIVE_SESSION_RESPONSE_INVALID');
  return {
    session: { id: nonempty(current.id, 200), userId: sessionUser, expiresAt: date(current.expiresAt), createdAt: date(current.createdAt), updatedAt: date(current.updatedAt) },
    user: { id: userId, name: text(user.name, 32768), email: text(user.email, 320), emailVerified: user.emailVerified,
      image: user.image === null ? null : text(user.image, 32768) },
  };
}
async function boundedJson(response: Response, signal: AbortSignal): Promise<unknown> {
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json' || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail('NATIVE_SESSION_RESPONSE_INVALID');
  }
  const reader = response.body.getReader(), chunks: Uint8Array[] = [];
  let total = 0;
  const cancel = () => { void reader.cancel().catch(() => undefined); };
  signal.addEventListener('abort', cancel, { once: true });
  try {
    if (signal.aborted) { cancel(); return fail('NATIVE_SESSION_ABORTED'); }
    for (;;) {
      const { value, done } = await reader.read();
      if (signal.aborted) return fail('NATIVE_SESSION_ABORTED');
      if (done) break;
      total += value.byteLength;
      if (total > 32 * 1024) { await reader.cancel(); return fail('NATIVE_SESSION_RESPONSE_LIMIT'); }
      chunks.push(value);
    }
  } finally { signal.removeEventListener('abort', cancel); reader.releaseLock(); }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  try { return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)); }
  catch { return fail('NATIVE_SESSION_RESPONSE_INVALID'); }
}

/** No environment discovery, cross-request cache, fallback backend or actor DTO. */
export type NativeSessionFetch = (input: URL, init: RequestInit) => Promise<Response>;
export type NativeSessionRead = (incoming: Pick<Headers, 'get'>, signal?: AbortSignal) => Promise<AccountSessionResponse | null>;
export function createNativeSessionClient(configuration: NativeSessionClientConfiguration, fetchPort: NativeSessionFetch = directSessionHttp): NativeSessionRead {
  const url = endpoint(configuration.privateBaseUrl);
  if (!Number.isInteger(configuration.timeoutMs) || configuration.timeoutMs < 250 || configuration.timeoutMs > 30000
    || configuration.holdingOrigins.length < 1 || configuration.holdingOrigins.length > 64) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  const origins = [configuration.accountOrigin, ...configuration.holdingOrigins].map(exactOrigin);
  const hosts = new Set(origins.map(origin => origin.host));
  if (hosts.size !== origins.length) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  const timeoutMs = configuration.timeoutMs;
  return async (incoming: Pick<Headers, 'get'>, signal?: AbortSignal): Promise<AccountSessionResponse | null> => {
    const host = incoming.get('host'), cookie = incoming.get('cookie');
    if (!host || !hosts.has(host)) return fail('NATIVE_SESSION_AUTHORITY_INVALID');
    if (cookie && new TextEncoder().encode(cookie).length > 8192) return fail('NATIVE_SESSION_COOKIE_LIMIT');
    const headers = new Headers({ host, accept: 'application/json', 'cache-control': 'no-cache' });
    if (cookie) headers.set('cookie', cookie);
    // Direct node:http never enters Next patched-fetch memo/HMR/data caches.
    // Public HTTPS authority never comes from private HTTP or forwarded headers.
    const controller = new AbortController();
    const cancelled = () => controller.abort();
    signal?.addEventListener('abort', cancelled, { once: true });
    if (signal?.aborted) controller.abort();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const response = await fetchPort(url, { method: 'GET', headers, cache: 'no-store', credentials: 'omit', redirect: 'error', signal: controller.signal });
      if (response.redirected) return fail('NATIVE_SESSION_REDIRECT_FORBIDDEN');
      if (response.status !== 200) return fail('NATIVE_SESSION_UNAVAILABLE', response.status);
      return session(await boundedJson(response, controller.signal));
    } catch (error) {
      const aborted = controller.signal.aborted;
      controller.abort();
      if (error instanceof NativeSessionReadError) throw error;
      return fail(aborted ? 'NATIVE_SESSION_ABORTED' : 'NATIVE_SESSION_UNAVAILABLE');
    } finally { clearTimeout(timer); signal?.removeEventListener('abort', cancelled); }
  };
}

/** A one-request, no-proxy/no-cookie-jar server transport. Native HTTP parsing
 * and stream cancellation stay in node:http; no custom HTTP parser or Next
 * internals are involved. No global agent/cache configuration is mutated. */
function directSessionHttp(url: URL, init: RequestInit): Promise<Response> {
  return new Promise((resolve, reject) => {
    // Next's ambient ProcessEnv type requires NODE_ENV even for ProxyEnv.
    // Node accepts this source-owned empty proxy object; no env is read/copied.
    // Node/Bun DOM stream declarations also differ in their BYOB overloads,
    // although Readable.toWeb supplies the standard stream used by Response.
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = request(url, {
      method: 'GET', agent, maxHeaderSize: 8192,
      headers: Object.fromEntries(new Headers(init.headers).entries()),
      signal: init.signal ?? undefined,
    }, incoming => {
      incoming.once('close', () => agent.destroy());
      try {
        const status = incoming.statusCode ?? 502;
        if (status !== 200) {
          incoming.destroy();
          resolve(new Response(null, { status: status >= 200 && status <= 599 ? status : 502 }));
          return;
        }
        const headers = new Headers();
        for (let index = 0; index < incoming.rawHeaders.length; index += 2) {
          headers.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
        }
        resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, { status, headers }));
      } catch (error) { incoming.destroy(); agent.destroy(); reject(error); }
    });
    outgoing.once('error', error => { agent.destroy(); reject(error); });
    outgoing.end();
  });
}
