import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativeFinanceReadAll,
  nativeFinanceReadFirst,
  FINANCE_RD_ASSET_POLICY_ROW,
  FINANCE_RD_ASSET_POLICY_RUNS,
  FINANCE_RD_INSTRUMENT_EDITION,
  FINANCE_RD_INSTRUMENT_RUNS,
  FINANCE_RD_COLLECTION_CASE_ROW,
  FINANCE_RD_BUSINESS_BILL_ROW,
} from '../../../packages/server-finance/src/adapters/native-finance-read';

const envKey = 'ERPX_NATIVE_FINANCE_READ_CONFIG';
const original = process.env[envKey];
beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44992',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
  });
});
afterEach(() => {
  if (original === undefined) delete process.env[envKey];
  else process.env[envKey] = original;
  vi.unstubAllGlobals();
});

const fakeDb = () => {
  const bound: unknown[][] = [];
  const prepared: string[] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
        async all() { return { results: [], success: true, meta: { changes: 0, duration: 0 } }; },
        async first() { return null; },
      };
    },
    async batch() { return [{ results: [{ id: 'row-1' }], success: true, meta: { changes: 0, duration: 0 } }]; },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, bound, prepared };
};

describe('native Finance caller-unit read plan', () => {
  it('exposes an allow-listed FINANCE_RD_* registry with no SQL on the wire', () => {
    expect(FINANCE_RD_ASSET_POLICY_ROW).toBe('finance_rd_asset_policy_row');
    expect(FINANCE_RD_ASSET_POLICY_RUNS).toBe('finance_rd_asset_policy_runs');
    expect(FINANCE_RD_COLLECTION_CASE_ROW).toBe('finance_rd_collection_case_row');
    expect(FINANCE_RD_BUSINESS_BILL_ROW).toBe('finance_rd_business_bill_row');
    for (const kind of [
      FINANCE_RD_ASSET_POLICY_ROW,
      FINANCE_RD_ASSET_POLICY_RUNS,
      FINANCE_RD_INSTRUMENT_EDITION,
      FINANCE_RD_INSTRUMENT_RUNS,
      FINANCE_RD_COLLECTION_CASE_ROW,
      FINANCE_RD_BUSINESS_BILL_ROW,
    ]) expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans a registered read and runs it on the caller unit with its binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS row', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const { db, bound, prepared } = fakeDb();

    expect(await nativeFinanceReadFirst(FINANCE_RD_COLLECTION_CASE_ROW, ['company', 'case-1'], db as never)).toEqual({ id: 'row-1' });
    expect(await nativeFinanceReadAll(FINANCE_RD_ASSET_POLICY_RUNS, ['company', 'policy-1'], db as never)).toMatchObject({ results: [{ id: 'row-1' }] });

    expect(bodies.map((b) => b.kind)).toEqual([FINANCE_RD_COLLECTION_CASE_ROW, FINANCE_RD_ASSET_POLICY_RUNS]);
    expect(bodies.map((b) => b.values)).toEqual([['company', 'case-1'], ['company', 'policy-1']]);
    expect(bound).toEqual([['company', 'case-1'], ['company', 'policy-1']]);
    expect(prepared).toEqual(['SELECT 1 AS row', 'SELECT 1 AS row']);

    const [url, init] = vi.mocked(fetch).mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/finance/command-plan');
    expect((init.headers as Record<string, string>).origin).toBe('https://acme.example.com');
    expect((init.headers as Record<string, string>)['x-erpx-context-company']).toBe('company');
  });

  it('plans the instrument fixed reads in Rust and runs them on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === FINANCE_RD_INSTRUMENT_EDITION
        ? 'SELECT * FROM finance_instrument_editions WHERE company_id=? AND id=?'
        : 'SELECT * FROM finance_instrument_runs WHERE company_id=? AND instrument_id=? ORDER BY ordinal,id';
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const { db, bound, prepared } = fakeDb();

    expect(await nativeFinanceReadFirst(FINANCE_RD_INSTRUMENT_EDITION, ['company', 'edition'], db as never)).toEqual({ id: 'row-1' });
    expect(await nativeFinanceReadAll(FINANCE_RD_INSTRUMENT_RUNS, ['company', 'edition'], db as never)).toMatchObject({ results: [{ id: 'row-1' }] });

    expect(bodies.map((b) => b.kind)).toEqual([FINANCE_RD_INSTRUMENT_EDITION, FINANCE_RD_INSTRUMENT_RUNS]);
    expect(bodies.map((b) => b.values)).toEqual([['company', 'edition'], ['company', 'edition']]);
    expect(bound).toEqual([['company', 'edition'], ['company', 'edition']]);
    expect(prepared).toEqual([
      'SELECT * FROM finance_instrument_editions WHERE company_id=? AND id=?',
      'SELECT * FROM finance_instrument_runs WHERE company_id=? AND instrument_id=? ORDER BY ordinal,id',
    ]);
  });

  it('fails closed when configuration is missing or the plan is malformed', async () => {
    delete process.env[envKey];
    const { db } = fakeDb();
    await expect(nativeFinanceReadFirst(FINANCE_RD_ASSET_POLICY_ROW, ['company', 'p'], db as never))
      .rejects.toMatchObject({ code: 'FINANCE_NATIVE_READ_UNAVAILABLE' });

    process.env[envKey] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44992', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
    });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ data: {} }), {
      status: 200, headers: { 'content-type': 'application/json' },
    })));
    await expect(nativeFinanceReadFirst(FINANCE_RD_ASSET_POLICY_ROW, ['company', 'p'], db as never))
      .rejects.toMatchObject({ code: 'FINANCE_NATIVE_READ_UNAVAILABLE' });
  });
});
