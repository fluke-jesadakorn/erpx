import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { directHttp } from '@erpx/server-identity_access/public/native-iam-assignments';

/**
 * Strict transport for the mounted native IAM session-list read.
 *
 * `POST /api/v1/holdings/{holding_slug}/identity-access/session-list-read`
 * executes the former TypeScript Control SELECT in Rust. The Host header and
 * `x-erpx-context-user` are the only authority inputs; the body is empty and
 * no caller-supplied user id is accepted. No SQL and no TypeScript fallback:
 * an absent, malformed, unregistered or failed native read throws 503.
 */
export const NATIVE_SESSION_LIST_ENV = 'ERPX_NATIVE_IDENTITY_ACCESS_CONFIG';

export type NativeSessionRow = {
  id: string;
  createdAt: string;
  expiresAt: string;
  userAgent: string | null;
};

export type NativeSessionListActor = Readonly<{
  userId: string;
  holdingId: string | null;
  /** Account-level reads (for example the IAM catch-all probe) are unbound by
   * Company; a registered Holding with a company binding still rejects them. */
  companyId?: string;
  platform?: boolean;
}>;

export type NativeSessionListConfiguration = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Readonly<{
    origin: string;
    host: string;
    holdingId: string;
    holdingSlug: string;
    companyId?: string;
  }>[];
}>;

export type NativeSessionListTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

const RESERVED = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const CONFIG_INVALID = 'NATIVE_IAM_SESSION_LIST_CONFIGURATION_INVALID';
const TARGET_INVALID = 'NATIVE_IAM_SESSION_LIST_TARGET_INVALID';
const UNAVAILABLE = 'NATIVE_IAM_SESSION_LIST_UNAVAILABLE';
const RESPONSE_INVALID = 'NATIVE_IAM_SESSION_LIST_RESPONSE_INVALID';
const REQUEST_INVALID = 'NATIVE_IAM_SESSION_LIST_REQUEST_INVALID';
const MAX_ROWS = 2000;

const fail = (status: number, code: string, message: string): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (message = 'The native IAM session read is unavailable.'): never =>
  fail(503, UNAVAILABLE, message);
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const id = (value: unknown, max = 200): value is string =>
  text(value, max) && /^[A-Za-z0-9_.:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  id(value, 128) && /^[a-z0-9_-]+$/.test(value);
const timestamp = (value: unknown): value is string =>
  text(value, 64) && (value as string).length >= 10 && (value as string).includes('T');

export function nativeSessionListConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[NATIVE_SESSION_LIST_ENV] !== undefined;
}

export function nativeSessionListConfiguration(raw: string): NativeSessionListConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw) as unknown;
  } catch {
    return fail(503, CONFIG_INVALID, 'The native IAM session transport is misconfigured.');
  }
  if (!record(value) || !text(value.privateBaseUrl, 512)) return fail(503, CONFIG_INVALID, 'The native IAM session transport is misconfigured.');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail(503, CONFIG_INVALID, 'The native IAM session transport is misconfigured.');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl ||
    !Number.isSafeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return fail(503, CONFIG_INVALID, 'The native IAM session transport is misconfigured.');
  const holdings: NativeSessionListConfiguration['holdings'][number][] = [];
  const hosts = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !id(item.holdingId) ||
      !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !id(item.companyId))
    )
      return fail(503, CONFIG_INVALID, 'The native IAM session transport is misconfigured.');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return fail(503, CONFIG_INVALID, 'The native IAM session transport is misconfigured.');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      hosts.has(origin.host)
    )
      return fail(503, CONFIG_INVALID, 'The native IAM session transport is misconfigured.');
    hosts.add(origin.host);
    holdings.push({
      origin: origin.origin,
      host: origin.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      ...(item.companyId === undefined ? {} : { companyId: item.companyId }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

type Holding = NativeSessionListConfiguration['holdings'][number];

function selectTarget(
  config: NativeSessionListConfiguration,
  actor: NativeSessionListActor,
  incoming: Pick<Headers, 'get'> | undefined,
): Holding {
  const host = incoming?.get('host') ?? null;
  let candidates: Holding[];
  if (host !== null) {
    if (!text(host, 255)) return fail(422, REQUEST_INVALID, 'Use one bounded Host.');
    candidates = config.holdings.filter((holding) => holding.host === host);
  } else {
    if (!id(actor.holdingId)) return fail(503, TARGET_INVALID, 'The native IAM session target is unknown.');
    candidates = config.holdings.filter((holding) => holding.holdingId === actor.holdingId);
  }
  if (candidates.length !== 1) return fail(503, TARGET_INVALID, 'The native IAM session target is unknown.');
  const target = candidates[0]!;
  if (actor.holdingId !== null && actor.holdingId !== target.holdingId)
    return fail(503, TARGET_INVALID, 'The native IAM session target is unknown.');
  if (target.companyId !== undefined && target.companyId !== actor.companyId)
    fail(409, 'WORK_CONTEXT_CHANGED', 'The selected Company changed. Reload and retry.');
  return target;
}

function row(value: unknown, seen: Set<string>): NativeSessionRow {
  if (!record(value)) return fail(502, RESPONSE_INVALID, 'The native IAM session reply was invalid.');
  const { id: sessionId, createdAt, expiresAt, userAgent } = value;
  if (
    Object.keys(value).length !== 4 ||
    !id(sessionId) ||
    seen.has(sessionId) ||
    !timestamp(createdAt) ||
    !timestamp(expiresAt) ||
    !(userAgent === null || text(userAgent, 1024))
  )
    return fail(502, RESPONSE_INVALID, 'The native IAM session reply was invalid.');
  seen.add(sessionId);
  return { id: sessionId, createdAt, expiresAt, userAgent };
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        return fail(502, RESPONSE_INVALID, 'The native IAM session reply was invalid.');
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.length;
  }
  return data;
}

export function createNativeSessionListClient(
  config: NativeSessionListConfiguration,
  transport: NativeSessionListTransport = directHttp,
) {
  return {
    async read(
      actor: NativeSessionListActor | undefined,
      incoming?: Pick<Headers, 'get'>,
    ): Promise<NativeSessionRow[]> {
      if (
        !actor ||
        !id(actor.userId) ||
        (actor.companyId !== undefined && !id(actor.companyId))
      )
        return fail(422, REQUEST_INVALID, 'Use a bounded IAM actor.');
      const target = selectTarget(config, actor, incoming);
      const headers = new Headers({
        host: target.host,
        accept: 'application/json',
        'content-type': 'application/json',
        'cache-control': 'no-cache',
        'x-erpx-context-user': actor.userId,
      });
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/identity-access/session-list-read`,
        config.base,
      );
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), config.timeoutMs);
      let response: Response;
      try {
        response = await transport('POST', url, headers, '{}', controller.signal);
      } catch {
        return unavailable();
      } finally {
        clearTimeout(timer);
      }
      if (response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return unavailable();
      }
      if ((response.headers.get('content-type') ?? '').split(';')[0]?.trim() !== 'application/json') {
        void response.body?.cancel().catch(() => undefined);
        return fail(502, RESPONSE_INVALID, 'The native IAM session reply was invalid.');
      }
      let payload: unknown;
      try {
        payload = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(await boundedBytes(response, 256 * 1024))) as unknown;
      } catch {
        return fail(502, RESPONSE_INVALID, 'The native IAM session reply was invalid.');
      }
      if (!record(payload) || Object.keys(payload).length !== 1 || !Array.isArray(payload.data) || payload.data.length > MAX_ROWS)
        return fail(502, RESPONSE_INVALID, 'The native IAM session reply was invalid.');
      const seen = new Set<string>();
      return payload.data.map((value) => row(value, seen));
    },
  };
}

/** Production entry: unconfigured and failed reads never touch TypeScript SQL. */
export async function nativeSessionList(
  actor: NativeSessionListActor | undefined,
  incoming?: Pick<Headers, 'get'>,
): Promise<NativeSessionRow[]> {
  const raw = process.env[NATIVE_SESSION_LIST_ENV];
  if (raw === undefined) return unavailable('The native IAM session transport is not configured.');
  return createNativeSessionListClient(nativeSessionListConfiguration(raw)).read(actor, incoming);
}
