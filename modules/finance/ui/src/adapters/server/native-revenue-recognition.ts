import 'server-only';
import {
  createNativeJournalWorkspaceClient,
  NativeFinanceReadError,
  type NativeFinanceReadConfiguration,
} from './native-journal-detail';
import { nativeFinanceCommandHttp, nativeFinanceReplyBytes } from './native-journal-post';
import { nativeFinanceConfiguration } from './native-journal-page';

const commandLimit = 64 * 1024;
const replyLimit = 256 * 1024;
const closed = (status: number, code: string, message: string) =>
  Response.json(
    { error: { code, message } },
    { status, headers: { 'Cache-Control': 'private, no-store' } },
  );

function configuration(): NativeFinanceReadConfiguration | undefined {
  const raw = process.env.ERPX_NATIVE_FINANCE_REVENUE_CONFIG;
  if (raw === undefined) return undefined;
  const result = nativeFinanceConfiguration(raw);
  createNativeJournalWorkspaceClient(result);
  return result;
}

function target(settings: NativeFinanceReadConfiguration, request: Request, holdingSlug: string) {
  const host = request.headers.get('host');
  return {
    host,
    holding: settings.holdings.find(
      (held) => new URL(held.origin).host === host && held.holdingSlug === holdingSlug,
    ),
  };
}

function copiedHeaders(request: Request, host: string): Headers {
  const headers = new Headers({ host, accept: 'application/json' });
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
    const value = request.headers.get(name);
    if (value !== null) headers.set(name, value);
  }
  const cookie = headers.get('cookie');
  if (cookie && new TextEncoder().encode(cookie).length > 8192) {
    throw new NativeFinanceReadError('NATIVE_FINANCE_COOKIE_LIMIT', 413);
  }
  return headers;
}

function object(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

function text(value: unknown, max = 200): value is string {
  return typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
}

function recognized(value: unknown): value is string {
  return typeof value === 'string' && /^(0|[1-9][0-9]*)\.[0-9]{2}$/.test(value);
}

async function reply(response: Response, max: number): Promise<unknown> {
  if (response.status >= 300 && response.status < 400) {
    void response.body?.cancel().catch(() => undefined);
    throw new NativeFinanceReadError('NATIVE_FINANCE_UNAVAILABLE', 502);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID', 502);
  }
  const payload = await nativeFinanceReplyBytes(response.body, max);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(payload));
  } catch {
    throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID', 502);
  }
}

function failure(value: unknown, status: number): Response {
  const error = (value as { error?: { code?: unknown; message?: unknown } })?.error;
  if (
    status < 400 ||
    status > 599 ||
    !error ||
    typeof error.code !== 'string' ||
    typeof error.message !== 'string' ||
    error.code.length > 100 ||
    error.message.length > 1000
  ) {
    return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm revenue recognition.');
  }
  return closed(status, error.code, error.message);
}

function recognizedData(value: unknown, obligationId: string): Record<string, unknown> | undefined {
  const data = (value as { data?: unknown })?.data;
  if (
    !object(data) ||
    data.id !== obligationId ||
    !text(data.journalId) ||
    !recognized(data.recognized)
  ) {
    return undefined;
  }
  return data;
}

/** Forward the exact recognition body to the native owner. An absent config
 * keeps the route's explicit native-unavailable branch; it never selects the
 * retired TypeScript writer. */
export async function nativeRevenueRecognition(
  request: Request,
  holdingSlug: string,
  obligationId: string,
): Promise<Response | undefined> {
  if (!obligationId || obligationId.includes('/') || obligationId.includes('\\')) {
    return closed(404, 'OBLIGATION_NOT_FOUND', 'Revenue obligation not found.');
  }
  const settings = configuration();
  if (!settings) return undefined;
  const { host, holding } = target(settings, request, holdingSlug);
  if (!holding || !host) return closed(404, 'NOT_FOUND', 'Not found.');
  const origin = request.headers.get('origin');
  if (origin !== null && origin !== holding.origin) {
    return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
  }
  const key = request.headers.get('idempotency-key');
  if (!key || key.length < 8 || key.length > 180) {
    return closed(422, 'IDEMPOTENCY_KEY_REQUIRED', 'Idempotency-Key is required.');
  }
  let body: Uint8Array;
  try {
    body = await nativeFinanceReplyBytes(request.body, commandLimit);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller recognition request.');
  }
  let headers: Headers;
  try {
    headers = copiedHeaders(request, host);
    headers.set('idempotency-key', key);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller recognition request.');
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/revenue-obligations/${encodeURIComponent(obligationId)}/recognize`,
    settings.privateBaseUrl,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, settings.timeoutMs);
  try {
    const response = await nativeFinanceCommandHttp(url, headers, body, controller.signal);
    const value = await reply(response, replyLimit);
    if (response.status === 200) {
      const data = recognizedData(value, obligationId);
      if (!data) {
        return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm revenue recognition.');
      }
      return Response.json(
        { data },
        { headers: { 'Cache-Control': 'private, no-store' } },
      );
    }
    return failure(value, response.status);
  } catch {
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm revenue recognition.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}
