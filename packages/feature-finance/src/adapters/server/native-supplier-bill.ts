import 'server-only';
import {
  createNativeJournalWorkspaceClient,
  NativeFinanceReadError,
  type NativeFinanceReadConfiguration,
} from './native-journal-detail';
import { nativeFinanceCommandHttp, nativeFinanceReplyBytes } from './native-journal-post';
import { nativeFinanceConfiguration, type NativeJournalPageContext } from './native-journal-page';

const limit = 128 * 1024;
const commandReplyLimit = 32 * 1024;
const detailReplyLimit = 2 * 1024 * 1024;
const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });

function configuration(): NativeFinanceReadConfiguration | undefined {
  const raw = process.env.ERPX_NATIVE_FINANCE_SUPPLIER_BILL_CONFIG;
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

function text(value: unknown, max = 4096): value is string {
  return typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
}

function integer(value: unknown, min = 0): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
}

function ledger(value: unknown): value is Record<string, unknown> {
  return (
    object(value) &&
    Object.values(value).every(
      (item) =>
        item === null ||
        typeof item === 'string' ||
        typeof item === 'boolean' ||
        (typeof item === 'number' && Number.isSafeInteger(item)),
    )
  );
}

function draft(value: unknown, id: string, key: string, version: number): value is Record<string, unknown> {
  if (!object(value)) return false;
  return (
    value.id === id &&
    text(value.number, 100) &&
    typeof value.total === 'string' &&
    typeof value.outstanding === 'string' &&
    value.status === 'draft' &&
    value.version === version &&
    (key.length === 0 || id === `invoice-${key}-bill`)
  );
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
    return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm the supplier bill.');
  }
  return closed(status, error.code, error.message);
}

async function command(
  request: Request,
  holdingSlug: string,
  path: string,
  validate: (value: unknown, key: string) => Record<string, unknown> | undefined,
  expectedStatus: number,
  rejectEvidence = false,
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
  if (rejectEvidence) {
    let value: unknown;
    try {
      value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
    } catch {
      return closed(422, 'INVALID_JSON', 'Use valid JSON.');
    }
    const evidence = object(value) ? value.attachmentSetIds : undefined;
    if (Array.isArray(evidence) && evidence.length > 0) {
      // Review-evidence binding still belongs to the legacy posting owner.
      return undefined;
    }
  }
  let headers: Headers;
  try {
    headers = copiedHeaders(request, host);
    headers.set('idempotency-key', key);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
  }
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/payables${path}`, settings.privateBaseUrl);
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
    const value = await reply(response, commandReplyLimit);
    if (response.status === expectedStatus) {
      const data = validate(value, key);
      if (!data) return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm the supplier bill.');
      return Response.json({ data }, { status: expectedStatus, headers: { 'Cache-Control': 'private, no-store' } });
    }
    return failure(value, response.status);
  } catch {
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to confirm the supplier bill.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}

export async function nativeSupplierBillCreate(
  request: Request,
  holdingSlug: string,
): Promise<Response | undefined> {
  return command(
    request,
    holdingSlug,
    '',
    (value, key) => {
      const data = (value as { data?: unknown })?.data;
      if (!draft(data, `invoice-${key}-bill`, key, 1)) return undefined;
      return data as Record<string, unknown>;
    },
    201,
  );
}

export async function nativeSupplierBillEdit(
  request: Request,
  holdingSlug: string,
  id: string,
): Promise<Response | undefined> {
  if (!id || id.includes('/') || id.includes('\\')) return closed(404, 'NOT_FOUND', 'Supplier bill not found.');
  return command(
    request,
    holdingSlug,
    `/${encodeURIComponent(id)}`,
    (value) => {
      const data = (value as { data?: unknown })?.data;
      if (
        !object(data) ||
        data.id !== id ||
        typeof data.total !== 'string' ||
        typeof data.outstanding !== 'string' ||
        data.status !== 'draft' ||
        !integer(data.version, 2)
      ) {
        return undefined;
      }
      return data;
    },
    200,
  );
}

export async function nativeSupplierBillPost(
  request: Request,
  holdingSlug: string,
  id: string,
): Promise<Response | undefined> {
  if (!id || id.includes('/') || id.includes('\\')) return closed(404, 'NOT_FOUND', 'Supplier bill not found.');
  const response = await command(
    request,
    holdingSlug,
    `/${encodeURIComponent(id)}/post`,
    (value, key) => {
      const data = (value as { data?: unknown })?.data;
      if (
        !object(data) ||
        data.id !== id ||
        data.status !== 'posted' ||
        data.journalId !== `journal-${key}`
      ) {
        return undefined;
      }
      return data;
    },
    200,
    true,
  );
  return response;
}

function validContext(
  value: unknown,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): value is Record<string, unknown> {
  if (!object(value) || !object(value.context)) return false;
  const context = value.context;
  const sameSet = (left: readonly string[], right: readonly string[]) => {
    const a = new Set(left);
    const b = new Set(right);
    return a.size === b.size && [...a].every((item) => b.has(item));
  };
  return (
    context.userId === actor.userId &&
    context.holdingId === actor.holdingId &&
    context.companyId === actor.companyId &&
    context.branchId === actor.branchId &&
    context.departmentId === actor.departmentId &&
    context.branchSlug === (branchSlug ?? null) &&
    context.departmentSlug === (departmentSlug ?? null) &&
    context.assignmentId === actor.assignmentId &&
    context.assignmentVersion === actor.authority?.assignmentVersion &&
    context.revision === actor.authority?.revision &&
    integer(context.expiresAt, 1) &&
    Array.isArray(context.modules) &&
    Array.isArray(context.permissions) &&
    sameSet(actor.modules, context.modules) &&
    sameSet(actor.iam?.permissions ?? [], context.permissions as string[])
  );
}

export async function nativeSupplierBillDetail(
  request: Request,
  actor: NativeJournalPageContext,
  holdingSlug: string,
  id: string,
): Promise<Response | undefined> {
  const settings = configuration();
  if (!settings) return undefined;
  if (!actor.authority || !actor.iam || actor.holdingSlug !== holdingSlug) {
    return closed(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review.');
  }
  if (!id || id.includes('/') || id.includes('\\')) return closed(404, 'NOT_FOUND', 'Supplier bill not found.');
  const { host, holding } = target(settings, request, holdingSlug);
  if (!holding || !host || holding.holdingId !== actor.holdingId) {
    return closed(404, 'NOT_FOUND', 'Not found.');
  }
  const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
  const departmentSlug = actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
  let headers: Headers;
  try {
    headers = copiedHeaders(request, host);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
  }
  headers.set('x-erpx-context-company', actor.companyId);
  headers.set('x-erpx-context-assignment', actor.assignmentId);
  headers.set('x-erpx-context-revision', String(actor.authority.revision));
  headers.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
  if (branchSlug) headers.set('x-erpx-workspace-branch', branchSlug);
  if (departmentSlug) headers.set('x-erpx-workspace-department', departmentSlug);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/payables/${encodeURIComponent(id)}`,
    settings.privateBaseUrl,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, settings.timeoutMs);
  try {
    const response = await nativeFinanceCommandHttp(url, headers, new Uint8Array(), controller.signal, 'GET');
    const value = await reply(response, detailReplyLimit);
    if (response.status !== 200) {
      if (response.status === 404) return undefined;
      return failure(value, response.status);
    }
    if (!validContext(value, actor, branchSlug, departmentSlug)) {
      return closed(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review.');
    }
    const data = value.data as Record<string, unknown>;
    if (
      !object(data) ||
      !ledger(data.invoice) ||
      data.invoice.id !== id ||
      data.invoice.kind !== 'payable' ||
      !Array.isArray(data.lines) ||
      !data.lines.every(ledger) ||
      !Array.isArray(data.lineHistory) ||
      !data.lineHistory.every(ledger) ||
      !Array.isArray(data.allocations) ||
      !data.allocations.every(ledger) ||
      !Array.isArray(data.thaiDocuments) ||
      !data.thaiDocuments.every(ledger) ||
      (data.journal !== null && !ledger(data.journal))
    ) {
      return closed(502, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to read the supplier bill.');
    }
    return Response.json({ data, context: value.context }, { headers: { 'Cache-Control': 'private, no-store' } });
  } catch {
    return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'Unable to read the supplier bill.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}
