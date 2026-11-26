import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeLane2Config } from '../native-lane-2/client';

/**
 * Strict transport for the native Finance invoice-list read.
 *
 * The owner executes the frozen Finance statement and returns every retired
 * column. This module authors no SQL, accepts no caller predicate, shapes no
 * payload and has no TypeScript fallback: a missing configuration, a changed
 * WorkContext or a malformed reply throws.
 */
const BODY_LIMIT = 2 * 1024 * 1024;
const MAX_ROWS = 101;

const REQUIRED_FIELDS = [
  'id',
  'invoice_number',
  'kind',
  'invoice_date',
  'due_date',
  'currency',
  'total_minor',
  'outstanding_minor',
  'status',
  'party_name',
] as const;

export type InvoiceRow = Readonly<Record<string, string | number | null>> & {
  readonly id: string;
  readonly invoice_number: string;
  readonly kind: string;
  readonly invoice_date: string | null;
  readonly due_date: string | null;
  readonly currency: string;
  readonly total_minor: number | null;
  readonly outstanding_minor: number | null;
  readonly status: string;
  readonly party_name: string | null;
};

type NativeActor = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;

export const fail = (status = 503): never => {
  throw new ApiError(
    status,
    'FINANCE_NATIVE_LANE_7_UNAVAILABLE',
    'The native Finance invoice-list read is unavailable.',
  );
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const validIdentifier = (value: string) =>
  value.length > 0 &&
  value.length <= 200 &&
  !/[\u0000-\u001f/?#\\]/.test(value);

function config() {
  try {
    return nativeLane2Config();
  } catch {
    return fail();
  }
}

function selectTarget(actor: NativeActor) {
  const resolved = config();
  if (!validIdentifier(actor.companyId)) return fail(422);
  const target = resolved.holdings.find(
    (holding) =>
      holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!target) throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The Holding context changed.');
  if (target.companyId !== undefined && target.companyId !== actor.companyId)
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The Holding company scope changed.');
  return { resolved, target };
}

async function readBody(body: ReadableStream<Uint8Array>): Promise<string> {
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > BODY_LIMIT) {
        await reader.cancel();
        return fail(502);
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder('utf-8', { fatal: true }).decode(data);
}

const scalar = (value: unknown) =>
  value === null || typeof value === 'string' || typeof value === 'number';

const parseInvoices = (payload: unknown): InvoiceRow[] => {
  if (!record(payload) || !Array.isArray(payload.data) || payload.data.length > MAX_ROWS)
    return fail(502);
  return payload.data.map((row: unknown) => {
    if (
      !record(row) ||
      !REQUIRED_FIELDS.every((field) => scalar(row[field])) ||
      !Object.values(row).every(scalar)
    )
      return fail(502);
    return row as InvoiceRow;
  });
};

/** Read the complete owner-owned invoice list for one bounded Company. */
export async function financeInvoiceListNative(
  actor: NativeActor,
  limit = 100,
): Promise<InvoiceRow[]> {
  const { resolved, target } = selectTarget(actor);
  if (!Number.isFinite(limit)) return fail(422);
  const bound = Math.max(1, Math.min(Math.trunc(limit), 100));
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/finance/native-lane-7/invoice-list?limit=${bound}`,
    resolved.base,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers: {
        host: target.host,
        origin: target.origin,
        accept: 'application/json',
        'cache-control': 'no-cache',
        'x-erpx-context-company': actor.companyId,
      },
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(resolved.timeoutMs),
    });
  } catch {
    return fail();
  }
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body.cancel().catch(() => undefined);
    return fail(502);
  }
  try {
    return parseInvoices(JSON.parse(await readBody(response.body)));
  } catch {
    return fail(502);
  }
}
