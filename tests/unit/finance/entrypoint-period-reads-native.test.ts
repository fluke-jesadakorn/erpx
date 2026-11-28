import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import { financeEntrypointQueries } from '../../../modules/finance/server/src/public/entrypoint-queries';
import {
  FINANCE_RD_PERIOD_END_DATES,
  FINANCE_RD_ALL_PERIODS,
} from '../../../modules/finance/server/src/adapters/native-finance-read';

const envKey = 'ERPX_NATIVE_FINANCE_READ_CONFIG';
const original = process.env[envKey];
beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44994',
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
  const prepared: string[] = [];
  const bound: unknown[][] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
        async all() { return { results: [{ id: 'row-1' }], success: true, meta: { changes: 0, duration: 0 } }; },
        async first() { return null; },
      };
    },
    async batch() { return [{ results: [{ id: 'period-1', start_date: '2026-01-01' }], success: true, meta: { changes: 0, duration: 0 } }]; },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, prepared, bound };
};

describe('native Finance entrypoint period reads', () => {
  it('exposes fixed read kinds with no SQL on the wire', () => {
    expect(FINANCE_RD_PERIOD_END_DATES).toBe('finance_rd_period_end_dates');
    expect(FINANCE_RD_ALL_PERIODS).toBe('finance_rd_all_periods');
    for (const kind of [FINANCE_RD_PERIOD_END_DATES, FINANCE_RD_ALL_PERIODS]) {
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
    }
  });

  it('plans both period reads in Rust and runs them on the caller unit with Company binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === FINANCE_RD_PERIOD_END_DATES
        ? 'SELECT id,end_date FROM finance_periods WHERE company_id=? ORDER BY start_date DESC'
        : 'SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC';
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const { db, prepared, bound } = fakeDb();
    const queries = financeEntrypointQueries(db as never);

    expect(await queries.periodEndDates('company')).toMatchObject({
      results: [{ id: 'period-1' }], success: true,
    });
    expect(await queries.allPeriods('company')).toEqual([{ id: 'period-1', start_date: '2026-01-01' }]);

    expect(bodies.map((b) => b.kind)).toEqual([FINANCE_RD_PERIOD_END_DATES, FINANCE_RD_ALL_PERIODS]);
    expect(bodies.map((b) => b.values)).toEqual([['company'], ['company']]);
    expect(bound).toEqual([['company'], ['company']]);
    expect(prepared).toEqual([
      'SELECT id,end_date FROM finance_periods WHERE company_id=? ORDER BY start_date DESC',
      'SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC',
    ]);

    const [url] = vi.mocked(fetch).mock.calls[0] as unknown as [URL];
    expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/finance/command-plan');
  });

  it('fails closed when the native Finance read service is unavailable', async () => {
    delete process.env[envKey];
    const { db } = fakeDb();
    await expect(financeEntrypointQueries(db as never).allPeriods('company'))
      .rejects.toMatchObject({ code: 'FINANCE_NATIVE_READ_UNAVAILABLE' });
  });
});
