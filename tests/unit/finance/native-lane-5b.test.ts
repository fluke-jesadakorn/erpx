import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';

vi.mock('server-only', () => ({}));

import {
  RecordDetailsFinanceOwnedFacts,
  FINANCE_LANE5B_RD_PARTY,
  FINANCE_LANE5B_RD_ROWS,
} from '../../../packages/server-finance/src/adapters/records-record-details-owned-facts';

const ENV = 'ERPX_NATIVE_FINANCE_CUSTOMER_READ_CONFIG';
const holdings = [
  { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
  { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
  { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
];
const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:3198', timeoutMs: 10000, holdings, ...overrides });
const context = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'company-1' };
const db = {} as never;
const ok = (kind: string, value: unknown) =>
  new Response(JSON.stringify({ kind, value }), { status: 200, headers: { 'content-type': 'application/json' } });
const party = { id: 'party-1', company_id: 'company-1', kind: 'customer', name: 'Acme' };
const chartRow = { id: 'chart-1', company_id: 'company-1', base_currency: 'THB' };
const period = { start_date: '2026-01-01', end_date: '2026-01-31' };
const journalRow = { id: 'line-1', journal_id: 'journal-1', journal_number: 'JV-1', entry_date: '2026-01-15', currency: 'THB', status: 'posted' };

describe('native Finance lane 5b record facts transport', () => {
  beforeEach(() => {
    process.env[ENV] = config();
  });
  afterEach(() => {
    delete process.env[ENV];
    vi.unstubAllGlobals();
  });

  it('forwards the exact Holding, company and read arguments for all four kinds', async () => {
    const responses = [
      ok(FINANCE_LANE5B_RD_PARTY, party),
      ok('record_facts_account', chartRow),
      ok('record_facts_period', period),
      ok(FINANCE_LANE5B_RD_ROWS, [journalRow]),
    ];
    vi.stubGlobal('fetch', vi.fn().mockImplementation(() => Promise.resolve(responses.shift())));
    const reads = RecordDetailsFinanceOwnedFacts(db, context);
    expect(await reads.readCommerceMasterDetailRow2('party-1', 'company-1')).toEqual(party);
    expect(await reads.readRecordSupplementDataAccount('chart-1', 'company-1')).toEqual(chartRow);
    expect(await reads.readRecordSupplementDataPeriod('period-1', 'company-1')).toEqual(period);
    expect(
      await reads.readRecordSupplementDataRows3('account-1', 'company-1', 7, '1', '2026-01-01', '2026-01-01', '2026-01-31', '2026-01-31', 30),
    ).toEqual({ results: [journalRow] });

    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls).toHaveLength(4);
    for (const [url, init] of calls as Array<[URL, RequestInit]>) {
      expect(url.toString()).toBe('http://127.0.0.1:3198/api/v1/holdings/acme/finance/record-facts');
      const headers = new Headers(init.headers as HeadersInit);
      expect(headers.get('host')).toBe('acme.jesadakorn.com');
      expect(headers.get('origin')).toBe('https://acme.jesadakorn.com');
      expect(headers.get('x-erpx-context-company')).toBe('company-1');
    }
    expect(JSON.parse(String(calls[3][1].body))).toEqual({
      kind: FINANCE_LANE5B_RD_ROWS,
      id: 'account-1',
      chart_id: '7',
      value: '1',
      value2: '2026-01-01',
      value3: '2026-01-01',
      value4: '2026-01-31',
      value5: '2026-01-31',
      offset: 30,
    });
  });

  it('selects every registered Holding without a first-holding default', async () => {
    const fetchMock = vi.fn().mockImplementation(() => Promise.resolve(ok(FINANCE_LANE5B_RD_PARTY, party)));
    vi.stubGlobal('fetch', fetchMock);
    for (const holding of holdings) {
      await RecordDetailsFinanceOwnedFacts(db, {
        holdingId: holding.holdingId,
        holdingSlug: holding.holdingSlug,
      }).readCommerceMasterDetailRow2('party-1', 'company-1');
      const [url, init] = fetchMock.mock.calls.at(-1) as [URL, RequestInit];
      expect(url.toString()).toContain(`/holdings/${holding.holdingSlug}/finance/record-facts`);
      expect(new Headers(init.headers as HeadersInit).get('host')).toBe(holding.host);
    }
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it('fails closed without context, on an unknown Holding and on a company mismatch', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const read = RecordDetailsFinanceOwnedFacts(db);
    await expect(read.readCommerceMasterDetailRow2('party-1', 'company-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_FINANCE_RECORD_FACTS_CONTEXT_REQUIRED',
    });
    const unknown = RecordDetailsFinanceOwnedFacts(db, { holdingId: 'other', holdingSlug: 'acme' });
    await expect(unknown.readCommerceMasterDetailRow2('party-1', 'company-1')).rejects.toMatchObject({
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
    });
    const mismatched = RecordDetailsFinanceOwnedFacts(db, { ...context, companyId: 'company-2' });
    await expect(mismatched.readCommerceMasterDetailRow2('party-1', 'company-1')).rejects.toMatchObject({
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('fails closed on a malformed native response', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ kind: FINANCE_LANE5B_RD_PARTY, value: [{ name: 'no id' }] }), { status: 200 }),
    ));
    await expect(
      RecordDetailsFinanceOwnedFacts(db, context).readCommerceMasterDetailRow2('party-1', 'company-1'),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_FINANCE_RECORD_FACTS_RESPONSE_INVALID' });
  });
});

describe('native Finance lane 5b SQL/result parity', () => {
  const srcPath = '../../../packages/server-finance/src/adapters/records-record-details-owned-facts.ts';
  const rsPath = '../../../packages/server-finance/src/adapters/native_lane_5/domain.rs';

  it('the TS adapter contains no SQL and no local fallback', () => {
    const src = readFileSync(new URL(srcPath, import.meta.url), 'utf8');
    expect(src).not.toMatch(/\bSELECT\b/);
    expect(src).not.toMatch(/\.prepare\(/);
    expect(src).not.toMatch(/tableByIdSql|PERIOD_DATES_SQL|fiscalCloseSourceSql/);
    expect(src).toContain('/finance/record-facts');
    expect(src).toContain('context?: FinanceRecordFactsContext');
  });

  it('pins the byte-identical statement heads executed by Rust', () => {
    const domain = readFileSync(new URL(rsPath, import.meta.url), 'utf8');
    const oracles: Record<string, string> = {
      RF_PARTY_SQL: '79689b31d9cfbff8f1bda82066aac43ad5bb25e894c5409433976e4e8c62a797',
      RF_ACCOUNT_SQL: '96f24f09d9e1b31101e112a94f6c4e4fa53df19c6e2d7463aecdc7c9e64ee6ee',
      RF_PERIOD_SQL: 'd447a713437d5c3dc293c50cdb1835d31131b3b3c1f2f181d36e3d1b0bbece3b',
      RF_ROWS_SQL: 'cb40a97c5a78297b84e46acb663e5a0687dd09c99b63fed75e47439f05c1cefc',
    };
    for (const [name, expected] of Object.entries(oracles)) {
      const match = domain.match(new RegExp(`pub const ${name}: &str = "([^"]+)"`));
      expect(match, name).not.toBeNull();
      expect(createHash('sha256').update(match![1]).digest('hex')).toBe(expected);
    }
  });
});
