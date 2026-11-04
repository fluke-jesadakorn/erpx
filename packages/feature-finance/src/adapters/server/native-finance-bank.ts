import 'server-only';
import type {NativeJournalPageContext} from './native-journal-page';
import {nativeFinanceConfiguration} from './native-journal-page';
import {nativeFinanceCommandHttp,nativeFinanceReplyBytes} from './native-journal-post';

const limit = 2 * 1024 * 1024;
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): value is string => typeof value === 'string' && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });

type Kind = 'import' | 'reconcile';
function kind(parts: readonly string[]): { kind: Kind; id?: string } | undefined {
  if (parts[0] === 'bank' && parts[1] === 'statements' && parts[2] === 'import' && parts.length === 3)
    return { kind: 'import' };
  if (parts[0] === 'bank' && parts[1] === 'transactions' && text(parts[2]) && parts[3] === 'reconcile' && parts.length === 4)
    return { kind: 'reconcile', id: parts[2] };
  return undefined;
}

/** Native bank import/reconcile transport. Missing configuration retains the
 * TypeScript path; a configured native failure never falls back. */
export function createNativeFinanceBankCommand(
  raw: string,
  transport: typeof nativeFinanceCommandHttp = nativeFinanceCommandHttp,
) {
  const settings = nativeFinanceConfiguration(raw);
  return async (
    request: Request,
    actor: NativeJournalPageContext,
    holdingSlug: string,
    parts: readonly string[],
    input: unknown,
  ): Promise<Response | undefined> => {
    const selected = kind(parts);
    if (!selected) return undefined;
    const host = request.headers.get('host');
    const target = settings.holdings.find(
      (holding) => holding.holdingSlug === holdingSlug && new URL(holding.origin).host === host,
    );
    if (!target || target.holdingId !== actor.holdingId || actor.holdingSlug !== holdingSlug || !actor.authority || !actor.iam)
      return closed(403, 'NATIVE_FINANCE_AUTHORITY_INVALID', 'The current Finance work context is unavailable.');
    const expected = {
      'x-erpx-context-company': actor.companyId,
      'x-erpx-context-assignment': actor.assignmentId,
      'x-erpx-context-revision': String(actor.authority.revision),
      'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
    };
    if (Object.entries(expected).some(([name, value]) => request.headers.get(name) !== value))
      return closed(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before submitting.');
    const origin = request.headers.get('origin');
    if (origin !== null && origin !== target.origin)
      return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
    const key = request.headers.get('idempotency-key');
    if (!key) return closed(422, 'IDEMPOTENCY_KEY_REQUIRED', 'Idempotency-Key is required.');
    const headers = new Headers({ host: host!, accept: 'application/json', ...expected });
    for (const name of ['cookie', 'origin', 'x-erpx-workspace-branch', 'x-erpx-workspace-department']) {
      const value = request.headers.get(name);
      if (value !== null) {
        if (Buffer.byteLength(value) > 8192) return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
        headers.set(name, value);
      }
    }
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', key);
    let body: Uint8Array;
    try {
      body = await nativeFinanceReplyBytes(request.body, limit);
    } catch {
      return closed(413, 'BODY_TOO_LARGE', 'Use a smaller request.');
    }
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), settings.timeoutMs);
    const segment = (value: string) => encodeURIComponent(value.toWellFormed());
    const base = `/api/v1/holdings/${segment(holdingSlug)}/finance`;
    const path =
      selected.kind === 'import'
        ? `${base}/bank/statements/import`
        : `${base}/bank/transactions/${segment(selected.id!)}/reconcile`;
    try {
      const response = await transport(new URL(path, settings.privateBaseUrl), headers, body, controller.signal, 'POST');
      if (
        response.redirected ||
        (response.status >= 300 && response.status < 400) ||
        response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json'
      )
        return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The bank workflow is unavailable. Your draft is retained.');
      const value: unknown = JSON.parse(
        new TextDecoder('utf-8', { fatal: true }).decode(await nativeFinanceReplyBytes(response.body, limit)),
      );
      if (response.status >= 400) {
        const error = object(value) && object(value.error) ? value.error : undefined;
        if (error && text(error.code) && text(error.message) && text(error.message) && error.message.length <= 1000)
          return closed(response.status, error.code, error.message);
        return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The bank workflow is unavailable. Your draft is retained.');
      }
      const data = object(value) ? value.data : undefined;
      if (!object(data)) return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The bank workflow is unavailable.');
      if (selected.kind === 'import') {
        if (
          data.id !== `statement-${key}` ||
          data.status !== 'imported' ||
          !integer(data.imported, 1) ||
          data.settlementPaymentId !== null
        )
          return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The bank workflow is unavailable.');
      } else if (
        data.id !== selected.id ||
        data.status !== 'matched' ||
        !integer(data.version, 2) ||
        data.matchedType !== 'payment'
      ) {
        return closed(502, 'NATIVE_FINANCE_RESPONSE_INVALID', 'The bank workflow is unavailable.');
      }
      return Response.json(value, {
        status: selected.kind === 'import' ? 201 : 200,
        headers: { 'Cache-Control': 'private, no-store' },
      });
    } catch {
      return closed(503, 'NATIVE_FINANCE_UNAVAILABLE', 'The bank workflow is unavailable. Your draft is retained.');
    } finally {
      clearTimeout(timer);
    }
  };
}

export function nativeFinanceBankCommand(
  request: Request,
  actor: NativeJournalPageContext,
  holdingSlug: string,
  parts: readonly string[],
  input: unknown,
) {
  const raw = process.env.ERPX_NATIVE_FINANCE_BANK_CONFIG;
  if (raw === undefined) return Promise.resolve(undefined);
  return createNativeFinanceBankCommand(raw)(request, actor, holdingSlug, parts, input);
}
