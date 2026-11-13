import 'server-only';
import {Agent, request} from 'node:http';
import {Readable} from 'node:stream';

type Target = {origin: string; holdingSlug: string; holdingId: string};
type Settings = {privateBaseUrl: string; timeoutMs: number; holdings: Target[]; production?: boolean};

const reserved = new Set([3000, 3001, 3100, 3103, 5432, 5433, 5435, 8080]);
const allowedPort = (port: number, production: boolean) =>
  Number.isInteger(port) &&
  port >= 1 &&
  port <= 65535 &&
  !(port >= 3200 && port <= 3299) &&
  !reserved.has(port) &&
  (production || port !== 3101);
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const closed = (status: number, code: string, message: string) =>
  Response.json({error: {code, message}}, {status, headers: {'Cache-Control': 'private, no-store'}});

function settings(raw: string): Settings {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw Error('NATIVE_INVENTORY_COUNT_CONFIGURATION_INVALID');
  }
  if (
    !object(value) ||
    (value.production !== undefined && typeof value.production !== 'boolean') ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30000 ||
    !Array.isArray(value.holdings) ||
    !value.holdings.length ||
    value.holdings.length > 64
  ) {
    throw Error('NATIVE_INVENTORY_COUNT_CONFIGURATION_INVALID');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw Error('NATIVE_INVENTORY_COUNT_CONFIGURATION_INVALID');
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    !allowedPort(Number(base.port), value.production === true) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  ) {
    throw Error('NATIVE_INVENTORY_COUNT_CONFIGURATION_INVALID');
  }
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  for (const entry of value.holdings) {
    if (
      !object(entry) ||
      typeof entry.origin !== 'string' ||
      typeof entry.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(entry.holdingSlug) ||
      typeof entry.holdingId !== 'string' ||
      !entry.holdingId ||
      new TextEncoder().encode(entry.holdingId).length > 200
    ) {
      throw Error('NATIVE_INVENTORY_COUNT_CONFIGURATION_INVALID');
    }
    let origin: URL;
    try {
      origin = new URL(entry.origin);
    } catch {
      throw Error('NATIVE_INVENTORY_COUNT_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== entry.origin ||
      origin.username ||
      origin.password ||
      !origin.hostname.includes('.') ||
      hosts.has(origin.host) ||
      slugs.has(entry.holdingSlug)
    ) {
      throw Error('NATIVE_INVENTORY_COUNT_CONFIGURATION_INVALID');
    }
    hosts.add(origin.host);
    slugs.add(entry.holdingSlug);
  }
  return value as Settings;
}

function selected(request_: Request, holdingSlug: string): {config: Settings; target: Target} | undefined {
  const raw = process.env.ERPX_NATIVE_INVENTORY_COUNT_CONFIG;
  if (raw === undefined) return undefined;
  const config = settings(raw);
  const target = config.holdings.find((held) => held.holdingSlug === holdingSlug);
  if (!target) return undefined;
  if (new URL(target.origin).host !== request_.headers.get('host')) {
    throw Error('NATIVE_INVENTORY_COUNT_HOST_INVALID');
  }
  return {config, target};
}

function headers(request_: Request, target: Target, host: string): Headers {
  const copied = new Headers({host, accept: 'application/json'});
  for (const name of [
    'cookie',
    'origin',
    'content-type',
    'idempotency-key',
    'x-erpx-context-company',
    'x-erpx-context-revision',
    'x-erpx-context-assignment',
    'x-erpx-context-assignment-version',
    'x-erpx-workspace-branch',
    'x-erpx-workspace-department',
  ]) {
    const value = request_.headers.get(name);
    if (value !== null) copied.set(name, value);
  }
  const cookie = copied.get('cookie');
  if (cookie && new TextEncoder().encode(cookie).length > 8192) {
    throw Error('NATIVE_INVENTORY_COUNT_COOKIE_LIMIT');
  }
  if (copied.get('origin') !== null && copied.get('origin') !== target.origin) {
    throw Error('ORIGIN_FORBIDDEN');
  }
  return copied;
}

async function post(
  url: URL,
  headers_: Headers,
  body: string,
  timeoutMs: number,
  signal: AbortSignal,
): Promise<{status: number; body: unknown}> {
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal.addEventListener('abort', cancel, {once: true});
  if (signal.aborted) controller.abort();
  const timer = setTimeout(cancel, timeoutMs);
  try {
    const response = await new Promise<Response>((resolve, reject) => {
      const agent = new Agent({
        keepAlive: false,
        maxSockets: 1,
        maxTotalSockets: 1,
        proxyEnv: {} as import('node:http').ProxyEnv,
      });
      const req = request(
        url,
        {
          method: 'POST',
          headers: Object.fromEntries(headers_),
          agent,
          signal: controller.signal,
          maxHeaderSize: 8192,
        },
        (incoming) => {
          incoming.once('close', () => agent.destroy());
          try {
            const replyHeaders = new Headers();
            for (let index = 0; index < incoming.rawHeaders.length; index += 2) {
              replyHeaders.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
            }
            resolve(
              new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
                status: incoming.statusCode ?? 502,
                headers: replyHeaders,
              }),
            );
          } catch (error) {
            incoming.destroy();
            agent.destroy();
            reject(error);
          }
        },
      );
      req.once('error', (error) => {
        agent.destroy();
        reject(error);
      });
      req.end(body);
    });
    if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
      void response.body?.cancel().catch(() => undefined);
      throw Error('NATIVE_INVENTORY_COUNT_RESPONSE_INVALID');
    }
    const reader = response.body!.getReader();
    const chunks: Uint8Array[] = [];
    let size = 0;
    const abort = () => {
      void reader.cancel().catch(() => undefined);
    };
    controller.signal.addEventListener('abort', abort, {once: true});
    try {
      for (;;) {
        const part = await reader.read();
        if (controller.signal.aborted) throw Error('NATIVE_INVENTORY_COUNT_TIMEOUT');
        if (part.done) break;
        size += part.value.length;
        if (size > 2 * 1024 * 1024) {
          await reader.cancel();
          throw Error('NATIVE_INVENTORY_COUNT_RESPONSE_LIMIT');
        }
        chunks.push(part.value);
      }
    } finally {
      controller.signal.removeEventListener('abort', abort);
      reader.releaseLock();
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.length;
    }
    let value: unknown;
    try {
      value = JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(bytes));
    } catch {
      throw Error('NATIVE_INVENTORY_COUNT_RESPONSE_INVALID');
    }
    return {status: response.status, body: value};
  } finally {
    clearTimeout(timer);
    signal.removeEventListener('abort', cancel);
  }
}

/**
 * Forward a validated draft Count create/revision to the native private API.
 * Returns `undefined` only when this deployment has no native Count target, so
 * the route fails closed instead of returning to a TypeScript writer.
 */
export async function nativeStockCountSave(
  request_: Request,
  holdingSlug: string,
  id: string | null,
  input: unknown,
): Promise<Response | undefined> {
  const choice = selected(request_, holdingSlug);
  if (!choice) return undefined;
  const key = request_.headers.get('idempotency-key');
  if (!key || key.length < 8 || key.length > 180) {
    return closed(422, 'COMMAND_KEY_REQUIRED', 'Idempotency-Key is required.');
  }
  let copied: Headers;
  try {
    copied = headers(request_, choice.target, new URL(choice.target.origin).host);
    copied.set('idempotency-key', key);
    copied.set('content-type', 'application/json');
  } catch (error) {
    if (error instanceof Error && error.message === 'ORIGIN_FORBIDDEN') {
      return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
    }
    return closed(413, 'NATIVE_INVENTORY_COUNT_REQUEST_INVALID', 'Use a smaller request.');
  }
  const suffix = id ? `/${encodeURIComponent(id)}` : '';
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/stock-counts${suffix}`,
    choice.config.privateBaseUrl,
  );
  let reply: {status: number; body: unknown};
  try {
    reply = await post(url, copied, JSON.stringify(input), choice.config.timeoutMs, request_.signal);
  } catch {
    return closed(503, 'NATIVE_INVENTORY_COUNT_UNAVAILABLE', 'The stock Count command is unavailable.');
  }
  const expectedStatus = id ? 200 : 201;
  if (reply.status !== expectedStatus) {
    const error = (reply.body as {error?: {code?: unknown; message?: unknown}} | null)?.error;
    if (
      reply.status >= 400 &&
      reply.status <= 599 &&
      error &&
      typeof error.code === 'string' &&
      typeof error.message === 'string'
    ) {
      return closed(reply.status, error.code, error.message);
    }
    return closed(502, 'NATIVE_INVENTORY_COUNT_UNAVAILABLE', 'The stock Count command is unavailable.');
  }
  const data = (reply.body as {data?: unknown} | null)?.data;
  const expected = id ?? `count-${key}`;
  if (
    !object(data) ||
    data.id !== expected ||
    data.status !== 'draft' ||
    typeof data.version !== 'number' ||
    !Number.isSafeInteger(data.version) ||
    data.version < 1
  ) {
    return closed(502, 'NATIVE_INVENTORY_COUNT_RESPONSE_INVALID', 'The stock Count command is unavailable.');
  }
  return Response.json({data}, {status: expectedStatus, headers: {'Cache-Control': 'private, no-store'}});
}

/**
 * Forward a validated generic Count transition to the native private API.
 * Returns `undefined` only when this deployment has no native Count target so
 * the route can keep the retained TypeScript transition until the mount lands.
 */
export async function nativeStockCountTransition(
  request_: Request,
  holdingSlug: string,
  id: string,
  action: 'submit' | 'reject' | 'cancel' | 'approve' | 'post',
  input: unknown,
): Promise<Response | undefined> {
  const choice = selected(request_, holdingSlug);
  if (!choice) return undefined;
  const key = request_.headers.get('idempotency-key');
  if (!key || key.length < 8 || key.length > 180) {
    return closed(422, 'COMMAND_KEY_REQUIRED', 'Idempotency-Key is required.');
  }
  let copied: Headers;
  try {
    copied = headers(request_, choice.target, new URL(choice.target.origin).host);
    copied.set('idempotency-key', key);
    copied.set('content-type', 'application/json');
  } catch (error) {
    if (error instanceof Error && error.message === 'ORIGIN_FORBIDDEN') {
      return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
    }
    return closed(413, 'NATIVE_INVENTORY_COUNT_REQUEST_INVALID', 'Use a smaller request.');
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/stock-counts/${encodeURIComponent(id)}/${action}`,
    choice.config.privateBaseUrl,
  );
  let reply: {status: number; body: unknown};
  try {
    reply = await post(url, copied, JSON.stringify(input), choice.config.timeoutMs, request_.signal);
  } catch {
    return closed(503, 'NATIVE_INVENTORY_COUNT_UNAVAILABLE', 'The stock Count command is unavailable.');
  }
  if (reply.status !== 200) {
    const error = (reply.body as {error?: {code?: unknown; message?: unknown}} | null)?.error;
    if (
      reply.status >= 400 &&
      reply.status <= 599 &&
      error &&
      typeof error.code === 'string' &&
      typeof error.message === 'string'
    ) {
      return closed(reply.status, error.code, error.message);
    }
    return closed(502, 'NATIVE_INVENTORY_COUNT_UNAVAILABLE', 'The stock Count command is unavailable.');
  }
  const data = (reply.body as {data?: unknown} | null)?.data;
  if (
    !object(data) ||
    data.id !== id ||
    typeof data.status !== 'string' ||
    typeof data.version !== 'number' ||
    !Number.isSafeInteger(data.version) ||
    data.version < 1
  ) {
    return closed(502, 'NATIVE_INVENTORY_COUNT_RESPONSE_INVALID', 'The stock Count command is unavailable.');
  }
  return Response.json({data}, {headers: {'Cache-Control': 'private, no-store'}});
}
