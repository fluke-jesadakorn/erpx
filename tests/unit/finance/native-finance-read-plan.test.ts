import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFile } from 'node:fs/promises';
vi.mock('server-only', () => ({}));
import { ACTIVE_CHART_BASE_CURRENCY_SQL } from '../../../packages/server-finance/src/adapters/chart-base-currency-dedupe-sql';
import { FINANCE_JOURNALS_PROJECTION_SQL } from '../../../packages/server-finance/src/adapters/finance-journals-projection-dedupe-sql';
import { FINANCE_CHARTS_PROJECTION_SQL } from '../../../packages/server-finance/src/adapters/finance-charts-projection-dedupe-sql';
import { finalRootOwnerProjectionSources } from '../../../packages/server-finance/src/adapters/final-root-owner-projection-sources';
import { platformOwnerProjectionSources } from '../../../packages/server-finance/src/adapters/platform-owner-projection-sources';
import { inventoryExecutiveOwnerProjections } from '../../../packages/server-finance/src/adapters/inventory-executive-owner-projections';
import { commercialOwnerProjectionSources } from '../../../packages/server-finance/src/adapters/commercial-owner-projection-sources';
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

const CHART_SQL = "SELECT base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'";
const CHART_CALL_SITES = [
  'packages/server-finance/src/adapters/finance-planning.ts',
  'packages/server-finance/src/adapters/accounting-profiles.ts',
  'packages/server-finance/src/adapters/payroll-recovery-chart.ts',
  'packages/server-finance/src/adapters/manufacturing-accounting-sources.ts',
  'packages/server-finance/src/adapters/refunds.ts',
];

describe('finance active-chart base_currency read dedupe', () => {
  it('keeps one byte-identical literal and adopts no share-lock variant', async () => {
    expect(ACTIVE_CHART_BASE_CURRENCY_SQL).toBe(CHART_SQL);
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/chart-base-currency-dedupe-sql.ts', import.meta.url), 'utf8');
    expect(shared.split(CHART_SQL).length - 1).toBe(1);
    expect(shared).not.toContain('FOR SHARE');
    for (const rel of CHART_CALL_SITES) {
      const text = await readFile(new URL(`../../../${rel}`, import.meta.url), 'utf8');
      expect(text.split(CHART_SQL).length - 1).toBe(0);
      expect(text).toContain('ACTIVE_CHART_BASE_CURRENCY_SQL');
      expect(text).not.toContain('FOR SHARE');
    }
  });

  it('drives the leaf reader through the shared SQL with unchanged binds', async () => {
    const prepares: string[] = [];
    const binds: unknown[][] = [];
    const db = {
      prepare(sql: string) {
        prepares.push(sql);
        return { bind(...values: unknown[]) { binds.push(values); return { first: async () => ({ base_currency: 'THB' }) }; } };
      },
    };
    const { payrollRecoveryChart } = await import('../../../packages/server-finance/src/adapters/payroll-recovery-chart');
    await expect(payrollRecoveryChart(db as never).readRecognizePayrollRecoveryChart('ch-1', 'co-1'))
      .resolves.toEqual({ base_currency: 'THB' });
    expect(prepares).toEqual([CHART_SQL]);
    expect(binds).toEqual([['ch-1', 'co-1']]);
  });
});

const POSTED_CHART_SQL = "SELECT 1 FROM finance_journals WHERE chart_id=? AND status IN ('posted','reversed')";

describe('finance books posted-chart existence read dedupe', () => {
  it('keeps one byte-identical literal and two constant-backed reads', async () => {
    const text = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-books.ts', import.meta.url), 'utf8');
    expect(text.split(`"${POSTED_CHART_SQL}"`).length - 1).toBe(1);
    expect(text.split('chartPostedSql,').length - 1).toBe(2);
  });
});

const JOURNALS_CALL_SITES = [
  'packages/server-finance/src/adapters/final-root-owner-projection-sources.ts',
  'packages/server-finance/src/adapters/platform-owner-projection-sources.ts',
  'packages/server-finance/src/adapters/inventory-executive-owner-projections.ts',
];

describe('finance finance_journals projection dedupe', () => {
  it('keeps one byte-identical literal and backs all three owner projections with it', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-journals-projection-dedupe-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('FROM finance_journals').length - 1).toBe(1);
    for (const rel of JOURNALS_CALL_SITES) {
      const text = await readFile(new URL(`../../../${rel}`, import.meta.url), 'utf8');
      expect(text.split('FROM finance_journals').length - 1).toBe(0);
      expect(text).toContain('FINANCE_JOURNALS_PROJECTION_SQL');
    }
    expect(finalRootOwnerProjectionSources.finance_journals).toBe(FINANCE_JOURNALS_PROJECTION_SQL);
    expect(platformOwnerProjectionSources.finance_journals).toBe(FINANCE_JOURNALS_PROJECTION_SQL);
    expect(inventoryExecutiveOwnerProjections.finance_journals).toBe(FINANCE_JOURNALS_PROJECTION_SQL);
  });
});

const CHARTS_CALL_SITES = [
  'packages/server-finance/src/adapters/platform-owner-projection-sources.ts',
  'packages/server-finance/src/adapters/inventory-executive-owner-projections.ts',
  'packages/server-finance/src/adapters/commercial-owner-projection-sources.ts',
];

describe('finance finance_charts projection dedupe', () => {
  it('keeps one byte-identical literal and backs all three owner projections with it', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-charts-projection-dedupe-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('FROM finance_charts').length - 1).toBe(1);
    for (const rel of CHARTS_CALL_SITES) {
      const text = await readFile(new URL(`../../../${rel}`, import.meta.url), 'utf8');
      expect(text.split('FROM finance_charts').length - 1).toBe(0);
      expect(text).toContain('FINANCE_CHARTS_PROJECTION_SQL');
    }
    expect(platformOwnerProjectionSources.finance_charts).toBe(FINANCE_CHARTS_PROJECTION_SQL);
    expect(inventoryExecutiveOwnerProjections.finance_charts).toBe(FINANCE_CHARTS_PROJECTION_SQL);
    expect(commercialOwnerProjectionSources.finance_charts).toBe(FINANCE_CHARTS_PROJECTION_SQL);
  });
});
