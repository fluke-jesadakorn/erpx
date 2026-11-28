import 'server-only';
import {
  createNativeJournalWorkspaceClient,
  NativeFinanceReadError,
  type NativeFinanceReadConfiguration,
} from './native-journal-detail';
import { nativeFinanceCommandHttp, nativeFinanceReplyBytes } from './native-journal-post';
import { nativeFinanceConfiguration } from './native-journal-page';

const limit = 2 * 1024 * 1024;
const replyLimit = 2 * 1024 * 1024;
const unavailable = 'The native Finance journal-batch service is unavailable.';
const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });

/**
 * The batch is native-only. A dedicated config may be supplied; otherwise the
 * existing journal-post registry is reused. Neither case can select TypeScript
 * business code: absent or invalid configuration fails closed.
 */
function configuration(): NativeFinanceReadConfiguration | undefined {
  const raw =
    process.env.ERPX_NATIVE_FINANCE_JOURNAL_BATCH_CONFIG ??
    process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG;
  if (raw === undefined) return undefined;
  const result = nativeFinanceConfiguration(raw);
  createNativeJournalWorkspaceClient(result);
  return result;
}

function object(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

function exact(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  return object(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

function integer(value: unknown, min = 0): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
}

function text(value: unknown, max: number): value is string {
  return typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
}

function batchResult(value: unknown, key: string): Record<string, unknown> | undefined {
  if (!exact(value, ['id', 'journals', 'lineCount', 'status'])) return undefined;
  if (value.id !== `journal-batch-${key}` || value.status !== 'posted') return undefined;
  if (!integer(value.lineCount, 0)) return undefined;
  if (!Array.isArray(value.journals) || value.journals.length < 1 || value.journals.length > 100) return undefined;
  for (const entry of value.journals) {
    if (!exact(entry, ['id', 'version'])) return undefined;
    if (!text(entry.id, 128) || !integer(entry.version, 1)) return undefined;
  }
  return value;
}

function copiedHeaders(request: Request, host: string, key: string): Headers {
  const headers = new Headers({ host, accept: 'application/json' });
  for (const name of [
    'cookie',
    'origin',
    'content-type',
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
  const cookie = headers.get('cookie');
  if (cookie && new TextEncoder().encode(cookie).length > 8192) {
    throw new NativeFinanceReadError('NATIVE_FINANCE_COOKIE_LIMIT', 413);
  }
  return headers;
}

async function reply(response: Response): Promise<unknown> {
  if (response.status >= 300 && response.status < 400) {
    void response.body?.cancel().catch(() => undefined);
    throw new NativeFinanceReadError('NATIVE_FINANCE_UNAVAILABLE', 502);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    throw new NativeFinanceReadError(
      'NATIVE_FINANCE_RESPONSE_INVALID',
      response.status >= 400 && response.status <= 599 ? response.status : 502,
    );
  }
  const payload = await nativeFinanceReplyBytes(response.body, replyLimit);
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
    !object(error) ||
    typeof error.code !== 'string' ||
    typeof error.message !== 'string' ||
    error.code.length > 100 ||
    error.message.length > 1000
  ) {
    return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', unavailable);
  }
  return closed(status, error.code, error.message);
}

/** One exact batch POST. Never returns undefined or selects a TypeScript writer. */
export async function nativeJournalBatch(request: Request, holdingSlug: string): Promise<Response> {
  try {
    const settings = configuration();
    if (!settings) return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', unavailable);
    const host = request.headers.get('host');
    if (!host) return closed(404, 'NOT_FOUND', 'Holding not found.');
    const holding = settings.holdings.find(
      (candidate) => new URL(candidate.origin).host === host && candidate.holdingSlug === holdingSlug,
    );
    if (!holding) return closed(404, 'NOT_FOUND', 'Holding not found.');
    const origin = request.headers.get('origin');
    if (origin !== null && origin !== holding.origin) {
      return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
    }
    const key = request.headers.get('idempotency-key');
    if (!key || key.length > 180) {
      return closed(422, 'IDEMPOTENCY_KEY_REQUIRED', 'Idempotency-Key is required.');
    }
    let body: Uint8Array;
    try {
      body = await nativeFinanceReplyBytes(request.body, limit);
    } catch {
      return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
    }
    let headers: Headers;
    try {
      headers = copiedHeaders(request, host, key);
    } catch {
      return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
    }
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/journals/batch-post`,
      settings.privateBaseUrl,
    );
    const controller = new AbortController();
    const cancel = () => controller.abort();
    request.signal.addEventListener('abort', cancel, { once: true });
    if (request.signal.aborted) controller.abort();
    const timer = setTimeout(cancel, settings.timeoutMs);
    try {
      const response = await nativeFinanceCommandHttp(url, headers, body, controller.signal);
      const value = await reply(response);
      if (response.status === 200) {
        const data = (value as { data?: unknown })?.data;
        const result = batchResult(data, key);
        if (!result) return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', unavailable);
        return Response.json({ data: result }, { headers: { 'Cache-Control': 'private, no-store' } });
      }
      return failure(value, response.status);
    } finally {
      clearTimeout(timer);
      request.signal.removeEventListener('abort', cancel);
    }
  } catch (error) {
    if (
      error instanceof NativeFinanceReadError &&
      error.status &&
      error.status >= 400 &&
      error.status <= 599
    ) {
      return closed(error.status, error.code, unavailable);
    }
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', unavailable);
  }
}
