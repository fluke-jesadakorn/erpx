import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import { headers } from 'next/headers';
import {
  directHttp,
  NativeFinanceReadError,
  type NativeFinanceReadConfiguration,
  type NativeFinanceReadTransport,
} from './native-journal-detail';
import { nativeFinanceConfiguration, type NativeJournalPageContext } from './native-journal-page';

type Row = Record<string, string | number | null>;
export type NativeSalesReceiptSnapshot = {
  schema: 'finance.customer-receipt.v1';
  documentNumber: string;
  companyName: string;
  customerName: string;
  customerCode: string;
  date: string;
  currency: string;
  amountMinor: string;
  paymentId: string;
  paymentVersion: number;
  bankReference: string;
  journalId: string;
  allocations: { invoiceId: string; invoiceNumber: string; amountMinor: string }[];
};
export type NativeSalesReceipt = {
  sha256: string;
  receipt: Row;
  snapshot: NativeSalesReceiptSnapshot;
  status: 'issued' | 'cancelled';
};

const fail = (code: string, status?: number): never => {
  throw new NativeFinanceReadError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};

function configuration(raw: string): NativeFinanceReadConfiguration {
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
    !integer(config.timeoutMs, 250) ||
    config.timeoutMs > 30000
  ) {
    return fail('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  return config;
}

function validateSnapshot(value: unknown): value is NativeSalesReceiptSnapshot {
  if (!object(value)) return false;
  const snapshot = value as Record<string, unknown>;
  return (
    snapshot.schema === 'finance.customer-receipt.v1' &&
    text(snapshot.documentNumber) &&
    text(snapshot.companyName) &&
    text(snapshot.customerName) &&
    typeof snapshot.customerCode === 'string' &&
    typeof snapshot.date === 'string' &&
    text(snapshot.currency, 3) &&
    /^(0|[1-9][0-9]{0,15})$/.test(String(snapshot.amountMinor)) &&
    text(snapshot.paymentId) &&
    integer(snapshot.paymentVersion, 1) &&
    typeof snapshot.bankReference === 'string' &&
    text(snapshot.journalId) &&
    Array.isArray(snapshot.allocations) &&
    snapshot.allocations.every(
      (row) =>
        object(row) &&
        text(row.invoiceId) &&
        text(row.invoiceNumber) &&
        /^(0|[1-9][0-9]{0,15})$/.test(String(row.amountMinor)),
    )
  );
}

function validate(
  value: unknown,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): NativeSalesReceipt {
  if (!object(value) || !object(value.context) || !object(value.data)) {
    return fail('NATIVE_FINANCE_SALES_RECEIPT_RESPONSE_INVALID');
  }
  const context = value.context;
  const data = value.data;
  if (
    context.userId !== actor.userId ||
    context.holdingId !== actor.holdingId ||
    context.companyId !== actor.companyId ||
    context.branchId !== actor.branchId ||
    context.departmentId !== actor.departmentId ||
    context.branchSlug !== (branchSlug ?? null) ||
    context.departmentSlug !== (departmentSlug ?? null) ||
    context.assignmentId !== actor.assignmentId ||
    context.assignmentVersion !== actor.authority?.assignmentVersion ||
    context.revision !== actor.authority?.revision ||
    !integer(context.expiresAt, 1) ||
    !sameSet(actor.modules, context.modules as string[]) ||
    !sameSet(actor.iam?.permissions ?? [], context.permissions as string[])
  ) {
    return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  }
  if (
    !object(data.receipt) ||
    typeof data.sha256 !== 'string' ||
    !/^[a-f0-9]{64}$/.test(data.sha256) ||
    !['issued', 'cancelled'].includes(String(data.status)) ||
    !validateSnapshot(data.snapshot)
  ) {
    return fail('NATIVE_FINANCE_SALES_RECEIPT_RESPONSE_INVALID');
  }
  return {
    sha256: data.sha256,
    receipt: data.receipt as Row,
    snapshot: data.snapshot,
    status: data.status as 'issued' | 'cancelled',
  };
}

function requestHeaders(
  incoming: Pick<Headers, 'get'>,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): Headers {
  const out = new Headers({ accept: 'application/json', 'cache-control': 'no-cache' });
  const host = incoming.get('host');
  if (host) out.set('host', host);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192) {
      return fail('NATIVE_FINANCE_COOKIE_LIMIT');
    }
    out.set('cookie', cookie);
  }
  const origin = incoming.get('origin');
  if (origin) out.set('origin', origin);
  out.set('x-erpx-context-company', actor.companyId);
  out.set('x-erpx-context-assignment', actor.assignmentId);
  out.set('x-erpx-context-revision', String(actor.authority!.revision));
  out.set('x-erpx-context-assignment-version', String(actor.authority!.assignmentVersion));
  if (branchSlug) out.set('x-erpx-workspace-branch', branchSlug);
  if (departmentSlug) out.set('x-erpx-workspace-department', departmentSlug);
  return out;
}

export function createNativeSalesReceiptReader(
  raw: string,
  transport: NativeFinanceReadTransport = directHttp,
) {
  const config = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeJournalPageContext,
    holdingSlug: string,
    id: string,
  ): Promise<NativeSalesReceipt | undefined> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== holdingSlug) {
      return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
    }
    const host = incoming.get('host');
    const holding = config.holdings.find(
      (entry) => new URL(entry.origin).host === host && entry.holdingSlug === holdingSlug,
    );
    if (!holding || holding.holdingId !== actor.holdingId) {
      return fail('NATIVE_FINANCE_AUTHORITY_INVALID');
    }
    const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
    const departmentSlug =
      actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
    const out = requestHeaders(incoming, actor, branchSlug, departmentSlug);
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/sales/receipts/${encodeURIComponent(id)}`,
      config.privateBaseUrl,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const response = await transport(url, out, controller.signal);
      if (response.status === 404) {
        void response.body?.cancel().catch(() => undefined);
        return undefined;
      }
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return fail('NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE', response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0] !== 'application/json') {
        return fail('NATIVE_FINANCE_SALES_RECEIPT_RESPONSE_INVALID');
      }
      return validate(await response.json(), actor, branchSlug, departmentSlug);
    } catch (error) {
      if (error instanceof NativeFinanceReadError) throw error;
      return fail(
        controller.signal.aborted
          ? 'NATIVE_FINANCE_SALES_RECEIPT_ABORTED'
          : 'NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  };
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

const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });

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

/** Generic standalone receipt issue. Field-owned receipts stay on their native interface. */
export async function nativeSalesReceiptIssue(
  request: Request,
  holdingSlug: string,
  paymentId: string,
): Promise<Response | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_SALES_RECEIPT_CONFIG;
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
  const key = request.headers.get('idempotency-key');
  if (!key || key.length < 8 || key.length > 180) {
    return closed(422, 'IDEMPOTENCY_KEY_REQUIRED', 'Idempotency-Key is required.');
  }
  let body: Uint8Array;
  try {
    body = await bytes(request.body, 32 * 1024);
  } catch {
    return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
  }
  const out = new Headers({ host: host!, accept: 'application/json', 'content-type': 'application/json' });
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
    if (value !== null) out.set(name, value);
  }
  out.set('idempotency-key', key);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/sales/receipts/${encodeURIComponent(paymentId)}/issue`,
    settings.privateBaseUrl,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, settings.timeoutMs);
  try {
    const response = await directPost(url, out, body, controller.signal);
    if (response.status >= 300 && response.status < 400) {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE', 'Unable to issue the receipt.');
    }
    if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE', 'Unable to issue the receipt.');
    }
    const payload = await bytes(response.body, 32768);
    let value: unknown;
    try {
      value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(payload));
    } catch {
      return closed(502, 'NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE', 'Unable to issue the receipt.');
    }
    if (response.status === 201) {
      const data = (value as { data?: unknown })?.data;
      if (
        !object(data) ||
        data.id !== `customer-receipt-${paymentId}`
      ) {
        return closed(502, 'NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE', 'Unable to issue the receipt.');
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
      return closed(502, 'NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE', 'Unable to issue the receipt.');
    }
    return closed(response.status, error.code, error.message);
  } catch {
    return closed(503, 'NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE', 'Unable to issue the receipt.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}

export async function nativeSalesReceipt(
  actor: NativeJournalPageContext | undefined,
  holdingSlug: string,
  id: string,
): Promise<NativeSalesReceipt | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_SALES_RECEIPT_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  return createNativeSalesReceiptReader(raw)(await headers(), actor, holdingSlug, id);
}

export async function nativeSalesReceiptExportReadback(
  request: Request,
  actor: NativeJournalPageContext,
  holdingSlug: string,
  id: string,
): Promise<Response | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_SALES_RECEIPT_CONFIG;
  if (raw === undefined) return undefined;
  const settings = configuration(raw);
  const host = request.headers.get('host');
  const target = settings.holdings.find(
    (holding) => new URL(holding.origin).host === host && holding.holdingSlug === holdingSlug,
  );
  if (!target) return closed(404, 'NOT_FOUND', 'Not found.');
  if (!actor.authority || !actor.iam || actor.holdingSlug !== holdingSlug) {
    return closed(409, 'NATIVE_FINANCE_CONTEXT_CHANGED', 'Refresh the receipt workspace.');
  }
  const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
  const departmentSlug =
    actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
  const out = requestHeaders(request.headers, actor, branchSlug, departmentSlug);
  out.set('accept', 'text/html');
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/sales/receipts/${encodeURIComponent(id)}/export`,
    settings.privateBaseUrl,
  );
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), settings.timeoutMs);
  try {
    const response = await directHttp(url, out, controller.signal);
    if (response.status === 404) {
      void response.body?.cancel();
      return undefined;
    }
    if (response.status !== 200) {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE', 'Unable to load the receipt.');
    }
    return new Response(response.body, {
      status: 200,
      headers: {
        'Cache-Control': 'private, no-store',
        'Content-Type': 'text/html; charset=utf-8',
        'Content-Security-Policy': "default-src 'none'; style-src 'unsafe-inline'",
        'X-Content-Type-Options': 'nosniff',
        'X-Document-SHA256': response.headers.get('x-document-sha256') ?? '',
      },
    });
  } catch {
    return closed(503, 'NATIVE_FINANCE_SALES_RECEIPT_UNAVAILABLE', 'Unable to load the receipt.');
  } finally {
    clearTimeout(timer);
  }
}
