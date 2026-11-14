import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import { NativeFinanceReadError } from './native-journal-detail';
import { nativeFinanceConfiguration } from './native-journal-page';

const limit = 512 * 1024;
const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });
const fail = (code: string, status?: number): never => {
  throw new NativeFinanceReadError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const money = (value: unknown): value is string =>
  typeof value === 'string' && /^-?(0|[1-9][0-9]*)\.[0-9]{2}$/.test(value);

async function bytes(body: ReadableStream<Uint8Array> | null, max: number): Promise<Uint8Array> {
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

function directPost(
  url: URL,
  headers: Headers,
  body: Uint8Array,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      { method: 'POST', agent, maxHeaderSize: 8192, headers: Object.fromEntries(headers), signal },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const responseHeaders = new Headers();
          for (let i = 0; i < incoming.rawHeaders.length; i += 2) {
            responseHeaders.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
          }
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: incoming.statusCode ?? 502,
              headers: responseHeaders,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}

function configuration(raw: string) {
  const config = nativeFinanceConfiguration(raw);
  let base: URL;
  try {
    base = new URL(config.privateBaseUrl);
  } catch {
    return fail('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    config.privateBaseUrl !== base.origin ||
    !Number.isSafeInteger(config.timeoutMs) ||
    config.timeoutMs < 250 ||
    config.timeoutMs > 30000
  ) {
    return fail('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  return config;
}

/** Forward the exact opening-balance preview/import body to the native owner. */
export async function nativeOpeningBalance(
  request: Request,
  holdingSlug: string,
  action: 'preview' | 'import',
): Promise<Response | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_OPENING_CONFIG;
  if (raw === undefined) return undefined;
  const settings = configuration(raw);
  const host = request.headers.get('host');
  const target = settings.holdings.find(
    (holding) => new URL(holding.origin).host === host && holding.holdingSlug === holdingSlug,
  );
  if (!target) return closed(404, 'NOT_FOUND', 'Not found.');
  const origin = request.headers.get('origin');
  if (origin !== null && origin !== target.origin) {
    return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
  }
  let key: string | null = null;
  if (action === 'import') {
    key = request.headers.get('idempotency-key');
    if (!key || key.length < 8 || key.length > 180) {
      return closed(422, 'IDEMPOTENCY_KEY_REQUIRED', 'Idempotency-Key is required.');
    }
  }
  let body: Uint8Array;
  try {
    body = await bytes(request.body, limit);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
  }
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'content-type': 'application/json',
  });
  for (const name of [
    'cookie',
    'origin',
    'x-erpx-context-company',
    'x-erpx-context-revision',
    'x-erpx-context-assignment',
    'x-erpx-context-assignment-version',
    'x-erpx-workspace-branch',
    'x-erpx-workspace-department',
  ]) {
    const value = request.headers.get(name);
    if (value !== null) headers.set(name, value);
  }
  if (key) headers.set('idempotency-key', key);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/opening-balances/${action}`,
    settings.privateBaseUrl,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, settings.timeoutMs);
  try {
    const response = await directPost(url, headers, body, controller.signal);
    if (response.status >= 300 && response.status < 400) {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_OPENING_UNAVAILABLE', 'Unable to confirm opening balances.');
    }
    if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_OPENING_UNAVAILABLE', 'Unable to confirm opening balances.');
    }
    const payload = await bytes(response.body, 256 * 1024);
    let value: unknown;
    try {
      value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(payload));
    } catch {
      return closed(502, 'NATIVE_FINANCE_OPENING_UNAVAILABLE', 'Unable to confirm opening balances.');
    }
    if (response.status === 200 || response.status === 201) {
      const data = object(value) ? value.data : undefined;
      const valid =
        object(data) &&
        (action === 'preview'
          ? text(data.previewToken, 64) &&
            Array.isArray(data.lines) &&
            money(data.debit) &&
            money(data.credit)
          : text(data.id, 200) && text(data.journalId, 200));
      if (!valid) {
        return closed(502, 'NATIVE_FINANCE_OPENING_UNAVAILABLE', 'Unable to confirm opening balances.');
      }
      return Response.json({ data }, { status: response.status, headers: { 'Cache-Control': 'private, no-store' } });
    }
    const error = object(value) && object(value.error) ? value.error : undefined;
    if (
      response.status < 400 ||
      response.status > 599 ||
      !error ||
      typeof error.code !== 'string' ||
      typeof error.message !== 'string'
    ) {
      return closed(502, 'NATIVE_FINANCE_OPENING_UNAVAILABLE', 'Unable to confirm opening balances.');
    }
    return closed(response.status, error.code, error.message);
  } catch {
    return closed(503, 'NATIVE_FINANCE_OPENING_UNAVAILABLE', 'Unable to confirm opening balances.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}
