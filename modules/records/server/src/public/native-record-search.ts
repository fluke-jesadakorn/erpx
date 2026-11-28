import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';

/**
 * Strict server-side transport for the native Records reference-search route.
 * It carries no SQL and offers no TypeScript fallback: the axum owner resolves
 * the Holding session cookie and current Work selection, executes the
 * `workspace_records` read and returns the exact legacy `data` array. A missing
 * transport, malformed reply or rejected request fails closed.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string; companyId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;
type Reply = Readonly<{ data?: unknown; error?: { code?: string; message?: string } }>;

const unavailable = (): never => {
  throw new ApiError(503, 'NATIVE_RECORDS_SEARCH_UNAVAILABLE', 'The native Records search is unavailable.');
};
const selectionRequired = (): never => {
  throw new ApiError(409, 'NATIVE_RECORDS_SEARCH_SELECTION_REQUIRED', 'The native Records search requires the current Holding selection.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const validId = (v: string): boolean => v.length > 0 && v.length <= 200 && !/[\x00-\x1f\x7f]/.test(v);
const validSlug = (v: string): boolean => /^[a-z0-9_-]{1,128}$/.test(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_RECORDS_CONFIG;
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  const timeoutMs = value.timeoutMs;
  if (typeof timeoutMs !== 'number' || !Number.isSafeInteger(timeoutMs) || timeoutMs < 250 || timeoutMs > 30000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port || reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password || value.privateBaseUrl !== base.origin) return unavailable();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' || typeof item.holdingId !== 'string' || typeof item.companyId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password || !/^[a-z0-9_-]+$/.test(item.holdingSlug)) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId, companyId: item.companyId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: value.privateBaseUrl, holdings, timeoutMs };
}

/** GET with the explicit Host header; Node fetch cannot override Host. */
function directGet(url: URL, headers: Record<string, string>, timeoutMs: number): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = httpRequest(url, {
      method: 'GET',
      agent,
      headers: { ...headers },
      signal: AbortSignal.timeout(timeoutMs),
      maxHeaderSize: 8192,
    }, incoming => {
      incoming.once('close', () => agent.destroy());
      try {
        const values = new Headers();
        for (let i = 0; i < incoming.rawHeaders.length; i += 2) values.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
        resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, { status: incoming.statusCode ?? 502, headers: values }));
      } catch (error) { incoming.destroy(); agent.destroy(); reject(error); }
    });
    outgoing.once('error', error => { agent.destroy(); reject(error); });
    outgoing.end();
  });
}

export function nativeRecordSearchAvailable(): boolean {
  return process.env.ERPX_NATIVE_RECORDS_CONFIG !== undefined;
}

/**
 * Returns the exact owner `data` array; the caller adds presentation links.
 * Only the real session cookie is read from the incoming request. The user,
 * modules and scope are re-resolved by Rust from that cookie and the expected
 * Work selection, never copied from caller-supplied header claims.
 */
export async function nativeRecordSearch(
  actor: WorkContext,
  q: string,
  requestHeaders: Pick<Headers, 'get'>,
  send: (url: URL, headers: Record<string, string>, timeoutMs: number) => Promise<Response> = directGet,
): Promise<Record<string, unknown>[]> {
  if (typeof q !== 'string' || q.length < 1 || q.length > 800 || q.includes('\0')) return unavailable();
  const config = configuration();
  const holding = config.holdings.find(h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId);
  if (!holding || !actor.companyId) return unavailable();

  const cookie = requestHeaders.get('cookie');
  const authority = actor.authority;
  const scope = actor.scope;
  if (
    !cookie || cookie.length > 8192 ||
    !validId(actor.companyId) || !validId(actor.assignmentId) ||
    !authority || !Number.isSafeInteger(authority.revision) || authority.revision < 0 ||
    !Number.isSafeInteger(authority.assignmentVersion) || authority.assignmentVersion < 1 ||
    !scope
  ) return selectionRequired();
  const branchSlug = scope.kind === 'branch' || scope.kind === 'department' ? scope.branchSlug : null;
  const departmentSlug = scope.kind === 'department' ? scope.departmentSlug : null;
  if ((branchSlug != null && !validSlug(branchSlug)) || (departmentSlug != null && !validSlug(departmentSlug))) return selectionRequired();

  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/records/native-search`, config.privateBaseUrl);
  url.searchParams.set('q', q);
  const headers: Record<string, string> = {
    host: new URL(holding.origin).host,
    origin: holding.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    cookie,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(authority.revision),
    'x-erpx-context-assignment-version': String(authority.assignmentVersion),
  };
  if (branchSlug) headers['x-erpx-workspace-branch'] = branchSlug;
  if (departmentSlug) headers['x-erpx-workspace-department'] = departmentSlug;

  let response: Response;
  try {
    response = await send(url, headers, config.timeoutMs);
  } catch { return unavailable(); }
  if (response.status !== 200 || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    if (response.status >= 400 && response.status < 500) {
      const payload = await response.json().catch((): unknown => null);
      const err = record(payload) ? (payload as Reply).error : undefined;
      throw new ApiError(response.status, err?.code ?? 'NATIVE_RECORDS_SEARCH_FAILED', err?.message ?? 'The native Records search was rejected.');
    }
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  const body: unknown = await response.json().catch((): unknown => null);
  if (!record(body) || !Array.isArray(body.data)) return unavailable();
  return body.data as Record<string, unknown>[];
}
