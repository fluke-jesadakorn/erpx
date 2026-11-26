import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeLane2Config } from '../native-lane-2/client';

/**
 * Strict transport for the native Finance chart-list read.
 *
 * The owner executes `SELECT * FROM finance_charts WHERE company_id=? ORDER BY
 * name` and returns every column of every row. This module authors no SQL,
 * accepts no predicate, shapes no payload and has no TypeScript fallback: a
 * missing configuration, a changed WorkContext or a malformed reply throws.
 */
const BODY_LIMIT = 2 * 1024 * 1024;

export type ChartRow = Readonly<Record<string, string | number | boolean | null>> & {
  readonly id: string;
  readonly name: string;
};

type NativeActor = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;

export const fail = (status = 503): never => {
  throw new ApiError(
    status,
    'FINANCE_NATIVE_LANE_6_UNAVAILABLE',
    'The native Finance chart-list read is unavailable.',
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

const parseCharts = (payload: unknown): ChartRow[] => {
  if (!record(payload) || !Array.isArray(payload.data)) return fail(502);
  return payload.data.map((row: unknown) => {
    if (
      !record(row) ||
      typeof row.id !== 'string' ||
      typeof row.name !== 'string' ||
      !Object.values(row).every((value) =>
        value === null ||
        typeof value === 'string' ||
        typeof value === 'number' ||
        typeof value === 'boolean')
    )
      return fail(502);
    return row as ChartRow;
  });
};

/** Read the full owner-owned chart list for one bounded Company. */
export async function financeChartListNative(
  actor: NativeActor,
  send?: (url: URL, init: RequestInit) => Promise<Response>,
): Promise<ChartRow[]> {
  const { resolved, target } = selectTarget(actor);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/finance/native-lane-6/chart-list`,
    resolved.base,
  );
  let response: Response;
  try {
    const init: RequestInit = {
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
    };
    response = send
      ? await send(url, init)
      : await nativeRequest(url, {
          method: 'GET',
          headers: Object.fromEntries(new Headers(init.headers).entries()),
          signal: init.signal ?? undefined,
        }, resolved.timeoutMs);
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
    return parseCharts(JSON.parse(await readBody(response.body)));
  } catch {
    return fail(502);
  }
}
