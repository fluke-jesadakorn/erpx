import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import { createNativeJournalWorkspaceClient, type NativeFinanceReadConfiguration } from './native-journal-detail';

const limit = 128 * 1024;
const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });

function config(): NativeFinanceReadConfiguration | undefined {
  const raw = process.env.ERPX_NATIVE_FINANCE_SALES_INVOICE_CREATE_CONFIG;
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

function validData(value: unknown, key: string): value is Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const data = value as Record<string, unknown>;
  if (
    data.id !== `invoice-${key}-invoice` ||
    data.status !== 'draft' ||
    data.version !== 1 ||
    typeof data.number !== 'string' ||
    data.number.length === 0 ||
    typeof data.total !== 'string' ||
    typeof data.outstanding !== 'string' ||
    !data.completionEvidence ||
    typeof data.completionEvidence !== 'object' ||
    Array.isArray(data.completionEvidence)
  ) {
    return false;
  }
  const evidence = data.completionEvidence as Record<string, unknown>;
  return (
    typeof evidence.note === 'string' &&
    typeof evidence.confirmedBy === 'string' &&
    Number.isSafeInteger(evidence.confirmedAt) &&
    evidence.invoiceRevision === 1
  );
}

/** Exact native POST only. An absent config selects the pre-existing legacy route. */
export async function nativeSalesInvoiceCreate(
  request: Request,
  holdingSlug: string,
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
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/sales/invoices`,
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
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm sales invoice creation.');
    }
    if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm sales invoice creation.');
    }
    const payload = await bytes(response.body, 32768);
    let value: unknown;
    try {
      value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(payload));
    } catch {
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm sales invoice creation.');
    }
    if (response.status === 201) {
      const data = (value as { data?: unknown })?.data;
      if (!validData(data, key)) {
        return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm sales invoice creation.');
      }
      return Response.json({ data }, { status: 201, headers: { 'Cache-Control': 'private, no-store' } });
    }
    const error = (value as { error?: { code?: unknown; message?: unknown } })?.error;
    if (
      response.status < 400 ||
      response.status > 599 ||
      !error ||
      typeof error.code !== 'string' ||
      typeof error.message !== 'string' ||
      error.code.length > 100 ||
      error.message.length > 1000
    ) {
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm sales invoice creation.');
    }
    return closed(response.status, error.code, error.message);
  } catch {
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm sales invoice creation.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}
