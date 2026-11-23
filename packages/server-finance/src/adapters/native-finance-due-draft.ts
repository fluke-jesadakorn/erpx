import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Thin native transport for the Finance due-draft schedule/decide commands.
 *
 * This carries no SQL and offers no TypeScript fallback: the Rust composition
 * owns the retained purchase boundary, the holding-scoped actor and the
 * caller-transaction receipt/audit/outbox effects. A missing transport, an
 * unregistered Holding or an invalid reply fails closed (503/502), never
 * falling back to the retired TypeScript writers.
 *
 * Header/origin/key discipline mirrors
 * `packages/server-finance/src/adapters/native-finance-owner-write.ts`:
 * exact registered Holding host/origin, one Idempotency-Key, Holding
 * audience. Account-session cookies and the captured work-context headers are
 * forwarded so the private backend re-authenticates the same session.
 */

type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

const CONFIG_KEY = 'ERPX_NATIVE_FINANCE_DUE_DRAFT_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const BODY_LIMIT = 2 * 1024 * 1024;

const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function text(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0;
}

function configuration(raw: string): Settings {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || typeof value.timeoutMs !== 'number' || !Array.isArray(value.holdings))
    throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
  if (!Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30000)
    throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin
  )
    throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
  const holdings = value.holdings.map((item: unknown): Holding => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' || typeof item.holdingId !== 'string')
      throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      item.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !text(item.holdingId) ||
      !/^[a-z0-9_-]+$/.test(item.holdingSlug)
    )
      throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  if (holdings.length < 1 || holdings.length > 64) throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
  return { privateBaseUrl: value.privateBaseUrl, holdings, timeoutMs: value.timeoutMs };
}

function settings(): Settings | undefined {
  const raw = process.env[CONFIG_KEY];
  if (raw === undefined) return undefined;
  return configuration(raw);
}

async function replyBytes(body: ReadableStream<Uint8Array> | null, max: number): Promise<Uint8Array> {
  if (!body) return new Uint8Array();
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      size += next.value.length;
      if (size > max) {
        await reader.cancel();
        throw new Error('BODY_LIMIT');
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const result = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}

async function forward(url: URL, headers: Headers, body: Uint8Array, timeoutMs: number, signal: AbortSignal): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  const onAbort = () => controller.abort();
  signal.addEventListener('abort', onAbort, { once: true });
  try {
    const response = await fetch(url, {
      method: 'POST',
      headers,
      body: body as unknown as BodyInit,
      cache: 'no-store',
      redirect: 'error',
      signal: controller.signal,
    });
    return response;
  } finally {
    clearTimeout(timer);
    signal.removeEventListener('abort', onAbort);
  }
}

function contextHeaders(request: Request, actor: WorkContext): Headers | Response {
  const expected: Record<string, string> = {
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? ''),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? ''),
  };
  if (!actor.authority || !actor.iam) return closed(403, 'NATIVE_FINANCE_AUTHORITY_INVALID', 'The current Finance work context is unavailable.');
  for (const [name, value] of Object.entries(expected)) {
    if (request.headers.get(name) !== value)
      return closed(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before submitting.');
  }
  const headers = new Headers({ host: '', accept: 'application/json' });
  for (const name of ['cookie', 'origin', 'content-type', 'idempotency-key', 'x-erpx-context-company', 'x-erpx-context-revision', 'x-erpx-context-assignment', 'x-erpx-context-assignment-version', 'x-erpx-workspace-branch', 'x-erpx-workspace-department']) {
    const value = request.headers.get(name);
    if (value !== null) {
      if (Buffer.byteLength(value) > 8192) return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
      headers.set(name, value);
    }
  }
  return headers;
}

async function command(
  request: Request,
  actor: WorkContext,
  holdingSlug: string,
  path: string,
  input: unknown,
  key: string,
  expect: (data: Record<string, unknown>, key: string) => boolean,
  success: number,
): Promise<Response> {
  let current: Settings;
  try {
    const found = settings();
    if (!found) return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'The native Finance due-draft service is unavailable.');
    current = found;
  } catch {
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'The native Finance due-draft service is unavailable.');
  }
  const host = request.headers.get('host');
  const target = current.holdings.find(
    holding => holding.holdingSlug === holdingSlug && new URL(holding.origin).host === host,
  );
  if (!target || target.holdingId !== actor.holdingId || actor.holdingSlug !== holdingSlug || !actor.authority || !actor.iam)
    return closed(403, 'NATIVE_FINANCE_AUTHORITY_INVALID', 'The current Finance work context is unavailable.');
  const origin = request.headers.get('origin');
  if (origin !== null && origin !== target.origin)
    return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
  const forwarded = contextHeaders(request, actor);
  if (forwarded instanceof Response) return forwarded;
  if (request.headers.get('idempotency-key') !== key)
    return closed(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before submitting.');
  let encoded: Uint8Array;
  try {
    encoded = new TextEncoder().encode(JSON.stringify(input));
  } catch {
    return closed(422, 'VALIDATION_FAILED', 'Invalid draft request.');
  }
  if (encoded.byteLength > BODY_LIMIT) return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
  forwarded.set('host', new URL(target.origin).host);
  forwarded.set('content-type', 'application/json');
  forwarded.set('idempotency-key', key);
  const url = new URL(path, current.privateBaseUrl);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, current.timeoutMs);
  try {
    const response = await forward(url, forwarded, encoded, current.timeoutMs, controller.signal);
    if (response.redirected || (response.status >= 300 && response.status < 400)) {
      void response.body?.cancel().catch(() => undefined);
      return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The Finance draft workflow is unavailable. Your draft is retained.');
    }
    if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
      void response.body?.cancel().catch(() => undefined);
      return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The Finance draft workflow is unavailable. Your draft is retained.');
    }
    let value: unknown;
    try {
      value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(await replyBytes(response.body, BODY_LIMIT)));
    } catch {
      return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The Finance draft workflow is unavailable. Your draft is retained.');
    }
    if (response.status >= 400) {
      const error = record(value) && record(value.error) ? value.error : undefined;
      if (error && text(error.code) && typeof error.message === 'string' && text(error.message) && error.message.length <= 1000)
        return closed(response.status, error.code, error.message);
      return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The Finance draft workflow is unavailable. Your draft is retained.');
    }
    const data = record(value) ? value.data : undefined;
    if (!record(data) || !expect(data, key))
      return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The Finance draft workflow is unavailable. Your draft is retained.');
    return Response.json(value, { status: success, headers: { 'Cache-Control': 'private, no-store' } });
  } catch {
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'The Finance draft workflow is unavailable. Your draft is retained.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}

function scheduled(data: Record<string, unknown>, key: string): boolean {
  return (
    data.id === `finance-due-${key}` &&
    data.status === 'scheduled' &&
    typeof data.version === 'number' &&
    Number.isSafeInteger(data.version) &&
    data.version === 1
  );
}

function decided(id: string, action: 'retry' | 'cancel', expectedVersion: number) {
  return (data: Record<string, unknown>): boolean =>
    data.id === id &&
    data.status === (action === 'retry' ? 'scheduled' : 'cancelled') &&
    data.version === expectedVersion + 1;
}

/** Native due-draft schedule transport. Unconfigured fails closed 503. */
export function nativeFinanceDueDraftSchedule(
  request: Request,
  actor: WorkContext,
  holdingSlug: string,
  input: unknown,
  key: string,
): Promise<Response> {
  const segment = (value: string) => encodeURIComponent(value.toWellFormed());
  return command(
    request,
    actor,
    holdingSlug,
    `/api/v1/holdings/${segment(holdingSlug)}/finance/due-drafts`,
    input,
    key,
    scheduled,
    201,
  );
}

/** Native due-draft decide transport. Unconfigured fails closed 503. */
export function nativeFinanceDueDraftDecide(
  request: Request,
  actor: WorkContext,
  holdingSlug: string,
  id: string,
  action: 'retry' | 'cancel',
  input: unknown,
  key: string,
): Promise<Response> {
  const segment = (value: string) => encodeURIComponent(value.toWellFormed());
  const expected = record(input) && typeof input.expectedVersion === 'number' ? input.expectedVersion : NaN;
  return command(
    request,
    actor,
    holdingSlug,
    `/api/v1/holdings/${segment(holdingSlug)}/finance/due-drafts/${segment(id)}/${action}`,
    input,
    key,
    (data, _key) => Number.isSafeInteger(expected) && decided(id, action, expected)(data),
    200,
  );
}
