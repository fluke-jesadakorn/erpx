import 'server-only';
import { createNativeJournalWorkspaceClient, NativeFinanceReadError, type NativeFinanceReadConfiguration } from './native-journal-detail';
import { nativeFinanceCommandHttp, nativeFinanceReplyBytes } from './native-journal-post';
import { nativeFinanceConfiguration } from './native-journal-page';

const limit = 128 * 1024;
const replyLimit = 2 * 1024 * 1024;
const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });

function configuration(): NativeFinanceReadConfiguration | undefined {
  const raw = process.env.ERPX_NATIVE_FINANCE_SUPPLIER_BATCH_CONFIG;
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

function integer(value: unknown, min = 0): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
}

async function reply(response: Response, max: number): Promise<unknown> {
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
  const payload = await nativeFinanceReplyBytes(response.body, max);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(payload));
  } catch {
    throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID', 502);
  }
}

function failure(value: unknown, status: number, fallback: string): Response {
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
    return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', fallback);
  }
  return closed(status, error.code, error.message);
}

type Validation = (value: unknown, key: string) => Record<string, unknown> | undefined;

async function command(
  request: Request,
  holdingSlug: string,
  path: string,
  validate: Validation,
  expectedStatus: number,
  fallbackMessage: string,
): Promise<Response | undefined> {
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
    body = await nativeFinanceReplyBytes(request.body, limit);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
  }
  let headers: Headers;
  try {
    headers = copiedHeaders(request, host);
    headers.set('idempotency-key', key);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/${path}`,
    settings.privateBaseUrl,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, settings.timeoutMs);
  try {
    const response = await nativeFinanceCommandHttp(url, headers, body, controller.signal);
    if (response.status === 404) {
      void response.body?.cancel().catch(() => undefined);
      return undefined;
    }
    const value = await reply(response, replyLimit);
    if (response.status === expectedStatus) {
      const data = validate(value, key);
      if (!data) return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', fallbackMessage);
      return Response.json({ data }, { status: expectedStatus, headers: { 'Cache-Control': 'private, no-store' } });
    }
    return failure(value, response.status, fallbackMessage);
  } catch (error) {
    if (error instanceof NativeFinanceReadError && error.status && error.status >= 400 && error.status <= 599) {
      return closed(error.status, error.code, fallbackMessage);
    }
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', fallbackMessage);
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}

function receipt(value: unknown, id: string, status: string, key: string): Record<string, unknown> | undefined {
  const data = (value as { data?: unknown })?.data;
  if (!object(data) || data.id !== id || data.status !== status) return undefined;
  if (!integer(data.version, 1)) return undefined;
  return data;
}

export async function nativeSupplierBatchCreate(
  request: Request,
  holdingSlug: string,
): Promise<Response | undefined> {
  return command(
    request,
    holdingSlug,
    'supplier-batches',
    (value, key) => {
      const data = (value as { data?: unknown })?.data;
      if (
        !object(data) ||
        data.id !== `supplier-batch-${key}` ||
        data.status !== 'draft' ||
        data.version !== 1
      ) {
        return undefined;
      }
      return data;
    },
    201,
    'Unable to confirm the supplier batch.',
  );
}

export async function nativeSupplierBatchDecision(
  request: Request,
  holdingSlug: string,
  id: string,
  decision: 'approve' | 'cancel',
): Promise<Response | undefined> {
  if (!id || id.includes('/') || id.includes('\\')) {
    return closed(404, 'NOT_FOUND', 'Supplier batch not found.');
  }
  return command(
    request,
    holdingSlug,
    `supplier-batches/${encodeURIComponent(id)}/${decision}`,
    (value, key) => {
      void key;
      const data = receipt(value, id, decision === 'approve' ? 'approved' : 'cancelled', '');
      return data;
    },
    200,
    'Unable to confirm the supplier decision.',
  );
}

function validExport(value: unknown): value is Record<string, unknown> & { content: string; sha256: string } {
  return (
    object(value) &&
    typeof value.content === 'string' &&
    value.content.length > 0 &&
    typeof value.sha256 === 'string' &&
    /^[a-f0-9]{64}$/.test(value.sha256)
  );
}

export async function verifySupplierBatchExport(value: unknown) {
  if (
    !validExport(value) ||
    !integer(value.rows, 1) ||
    value.confirmationRequired !== true
  ) {
    throw Error('Supplier export acknowledgement is incomplete.');
  }
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value.content));
  const actual = [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  if (actual !== value.sha256) {
    throw Error('The supplier export does not match its retained SHA-256. Retry the original export.');
  }
  return value;
}

export async function nativeSupplierBatchExport(
  request: Request,
  holdingSlug: string,
  id: string,
): Promise<Response | undefined> {
  if (!id || id.includes('/') || id.includes('\\')) {
    return closed(404, 'NOT_FOUND', 'Supplier batch not found.');
  }
  return command(
    request,
    holdingSlug,
    `supplier-batches/${encodeURIComponent(id)}/export`,
    (value) => {
      const data = (value as { data?: unknown })?.data;
      if (!object(data) || data.id !== id || !validExport(data) || !integer(data.rows, 1)) return undefined;
      if (data.confirmationRequired !== true) return undefined;
      return data;
    },
    200,
    'Unable to confirm the supplier export.',
  );
}

export async function nativeSupplierInstructionResult(
  request: Request,
  holdingSlug: string,
  id: string,
  input: { outcome?: unknown },
): Promise<Response | undefined> {
  if (!id || id.includes('/') || id.includes('\\')) {
    return closed(404, 'NOT_FOUND', 'Supplier instruction not found.');
  }
  const outcome = input.outcome;
  if (outcome !== 'unknown' && outcome !== 'failed') return undefined;
  return command(
    request,
    holdingSlug,
    `supplier-instructions/${encodeURIComponent(id)}/result`,
    (value) => receipt(value, id, outcome, ''),
    200,
    'Unable to confirm the supplier bank result.',
  );
}

export async function nativeSupplierInstructionConfirm(
  request: Request,
  holdingSlug: string,
  id: string,
): Promise<Response | undefined> {
  if (!id || id.includes('/') || id.includes('\\')) {
    return closed(404, 'NOT_FOUND', 'Supplier instruction not found.');
  }
  return command(
    request,
    holdingSlug,
    `supplier-instructions/${encodeURIComponent(id)}/confirm`,
    (value, key) => {
      const data = (value as { data?: unknown })?.data;
      if (!object(data) || data.id !== id || data.status !== 'confirmed') return undefined;
      if (!integer(data.version, 1)) return undefined;
      if (data.paymentId !== `supplier-payment-${key}`) return undefined;
      if (typeof data.journalId !== 'string' || data.journalId !== `journal-${key}`) return undefined;
      return data;
    },
    200,
    'Unable to confirm the supplier payment.',
  );
}

/** Recover the exact retained export acknowledgement for a batch. */
export async function nativeSupplierBatchExportRecovery(
  request: Request,
  holdingSlug: string,
  id: string,
): Promise<Response | undefined> {
  const settings = configuration();
  if (!settings) return undefined;
  const { host, holding } = target(settings, request, holdingSlug);
  if (!holding || !host) return closed(404, 'NOT_FOUND', 'Not found.');
  if (!id || id.includes('/') || id.includes('\\')) {
    return closed(404, 'NOT_FOUND', 'Supplier batch not found.');
  }
  const origin = request.headers.get('origin');
  if (origin !== null && origin !== holding.origin) {
    return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
  }
  let headers: Headers;
  try {
    headers = copiedHeaders(request, host);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/supplier-batches/${encodeURIComponent(id)}/export`,
    settings.privateBaseUrl,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, settings.timeoutMs);
  try {
    const response = await nativeFinanceCommandHttp(url, headers, new Uint8Array(), controller.signal, 'GET');
    if (response.status === 404) {
      void response.body?.cancel().catch(() => undefined);
      return undefined;
    }
    const value = await reply(response, replyLimit);
    if (response.status !== 200) {
      return failure(value, response.status, 'Unable to recover the supplier export.');
    }
    const data = (value as { data?: unknown })?.data;
    if (!object(data) || data.id !== id || !validExport(data) || !integer(data.rows, 1)) {
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to recover the supplier export.');
    }
    return Response.json(
      { data },
      { headers: { 'Cache-Control': 'private, no-store' } },
    );
  } catch (error) {
    if (error instanceof NativeFinanceReadError && error.status && error.status >= 400 && error.status <= 599) {
      return closed(error.status, error.code, 'Unable to recover the supplier export.');
    }
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to recover the supplier export.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}
