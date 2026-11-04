import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import { createNativeJournalWorkspaceClient, type NativeFinanceReadConfiguration } from './native-journal-detail';

const limit = 128 * 1024;
const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });

function config(): NativeFinanceReadConfiguration | undefined {
  const raw = process.env.ERPX_NATIVE_FINANCE_SALES_INVOICE_EDIT_CONFIG;
  if (raw === undefined) return undefined;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  const v = value as Record<string, unknown>;
  if (
    typeof v.privateBaseUrl !== 'string' ||
    typeof v.timeoutMs !== 'number' ||
    !Array.isArray(v.holdings) ||
    !v.holdings.every(
      (holding: unknown) =>
        !!holding &&
        typeof holding === 'object' &&
        !Array.isArray(holding) &&
        typeof (holding as Record<string, unknown>).origin === 'string' &&
        typeof (holding as Record<string, unknown>).holdingSlug === 'string' &&
        typeof (holding as Record<string, unknown>).holdingId === 'string',
    )
  ) {
    throw new Error('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  const result = v as NativeFinanceReadConfiguration;
  createNativeJournalWorkspaceClient(result);
  return result;
}

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

function direct(url: URL, headers: Headers, body: Uint8Array, signal: AbortSignal): Promise<Response> {
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

function object(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

function validEdit(value: unknown, id: string, expectedVersion: number): value is Record<string, unknown> {
  if (!object(value) || !object(value.data)) return false;
  const data = value.data;
  return (
    data.id === id &&
    data.status === 'draft' &&
    data.version === expectedVersion + 1 &&
    typeof data.total === 'string' &&
    typeof data.outstanding === 'string'
  );
}

function validVoid(value: unknown, id: string, expectedVersion: number): value is Record<string, unknown> {
  if (!object(value) || !object(value.data)) return false;
  const data = value.data;
  return data.id === id && data.status === 'voided' && data.version === expectedVersion + 1;
}

async function command(
  request: Request,
  holdingSlug: string,
  id: string,
  voiding: boolean,
): Promise<Response | undefined> {
  const settings = config();
  if (!settings) return undefined;
  const host = request.headers.get('host');
  const target = settings.holdings.find(
    (holding) => new URL(holding.origin).host === host && holding.holdingSlug === holdingSlug,
  );
  if (!target) return closed(404, 'NOT_FOUND', 'Not found.');
  const origin = request.headers.get('origin');
  if (origin !== null && origin !== target.origin) {
    return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
  }
  const key = request.headers.get('idempotency-key');
  if (!key || key.length < 8 || key.length > 180) {
    return closed(422, 'IDEMPOTENCY_KEY_REQUIRED', 'Idempotency-Key is required.');
  }
  let body: Uint8Array;
  try {
    body = await bytes(request.body, limit);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
  } catch {
    return closed(422, 'INVALID_SALES_INVOICE', 'Use a valid sales invoice request.');
  }
  if (!object(parsed)) {
    return closed(422, 'INVALID_SALES_INVOICE', 'Use a valid sales invoice request.');
  }
  const expectedVersion = parsed.expectedVersion;
  if (typeof expectedVersion !== 'number' || !Number.isSafeInteger(expectedVersion) || expectedVersion < 1) {
    return closed(422, 'INVALID_SALES_INVOICE', 'expectedVersion is required.');
  }
  // The generic editor also edits payable invoices; those stay on their owner.
  if (!voiding && parsed.kind !== 'receivable') return undefined;
  const headers = new Headers({ host: host!, accept: 'application/json', 'content-type': 'application/json' });
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
  headers.set('idempotency-key', key);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/invoices/${encodeURIComponent(id)}${voiding ? '/void' : ''}`,
    settings.privateBaseUrl,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, settings.timeoutMs);
  try {
    const response = await direct(url, headers, body, controller.signal);
    if (response.status >= 300 && response.status < 400) {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm the sales invoice.');
    }
    if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm the sales invoice.');
    }
    const payload = await bytes(response.body, 32768);
    let value: unknown;
    try {
      value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(payload));
    } catch {
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm the sales invoice.');
    }
    if (response.status === 200) {
      const data = object(value) ? value.data : undefined;
      if (voiding ? validVoid(value, id, expectedVersion) : validEdit(value, id, expectedVersion)) {
        return Response.json({ data }, { status: 200, headers: { 'Cache-Control': 'private, no-store' } });
      }
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm the sales invoice.');
    }
    const error = object(value) && object(value.error) ? value.error : undefined;
    if (response.status === 404 && error?.code === 'NATIVE_INVOICE_NOT_OWNED') {
      return undefined;
    }
    if (
      response.status < 400 ||
      response.status > 599 ||
      !error ||
      typeof error.code !== 'string' ||
      typeof error.message !== 'string' ||
      error.code.length > 100 ||
      error.message.length > 1000
    ) {
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm the sales invoice.');
    }
    return closed(response.status, error.code, error.message);
  } catch {
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm the sales invoice.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}

export const nativeSalesInvoiceEdit = (request: Request, holdingSlug: string, id: string) =>
  command(request, holdingSlug, id, false);
export const nativeSalesInvoiceVoid = (request: Request, holdingSlug: string, id: string) =>
  command(request, holdingSlug, id, true);
