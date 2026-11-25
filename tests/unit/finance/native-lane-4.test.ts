import { createHash } from 'node:crypto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { financeReferenceOptions, nativeFinanceLane4Configured, type Lane4Actor } from '../../../packages/server-finance/src/adapters/native-lane-4/client';
import { pmsFinanceFacts } from '../../../packages/server-finance/src/public/pms-command-facts';

const ENV = 'ERPX_NATIVE_FINANCE_LANE4_CONFIG';
const original = process.env[ENV];
const sha256 = (value: string) => createHash('sha256').update(value).digest('hex');

/** Retired TS renderings, pinned as the SQL oracle. The third is produced by
 * `finPartyIdNameByCompanySql` in `finance-read-tail-r57-sql.ts`. */
const SQL_ORACLE = {
  accounts: "SELECT id,name,code,chart_id,category FROM finance_accounts WHERE company_id=? AND status='active' AND currency='THB' ORDER BY code",
  charts: "SELECT id,name FROM finance_charts WHERE company_id=? AND base_currency='THB' AND status='active'",
  parties: "SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND currency='THB' AND person_id IS NULL",
} as const;

const ROWS = {
  accounts: [{ id: 'acct-1', name: 'Cash', code: '100', chart_id: 'chart-1', category: 'asset' }],
  charts: [{ id: 'chart-1', name: 'Main' }],
  parties: [{ id: 'party-1', name: 'Acme' }],
} as const;

const HOLDINGS = [
  { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co' },
  { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
  { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
] as const;
const config = { privateBaseUrl: 'http://127.0.0.1:3198', timeoutMs: 10000, holdings: HOLDINGS };
const actor = (i: 0 | 1 | 2): Lane4Actor => ({
  holdingId: HOLDINGS[i].holdingId,
  holdingSlug: HOLDINGS[i].holdingSlug,
  companyId: 'acme-co',
});

beforeEach(() => {
  process.env[ENV] = JSON.stringify(config);
});

afterEach(() => {
  if (original === undefined) delete process.env[ENV];
  else process.env[ENV] = original;
  vi.unstubAllGlobals();
});

function okReply(kind: keyof typeof ROWS): Response {
  return new Response(JSON.stringify({ data: ROWS[kind] }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

function stubFetch() {
  const calls: { url: string; headers: Headers }[] = [];
  vi.stubGlobal('fetch', vi.fn(async (url: URL, init: RequestInit) => {
    calls.push({ url: url.toString(), headers: new Headers(init.headers) });
    const kind = new Headers(init.headers).get('x-erpx-lane4-kind') as keyof typeof ROWS;
    return okReply(kind);
  }));
  return calls;
}

describe('native Finance lane-4 reference options', () => {
  it('has no SQL on the wire and pins the retired SQL/result oracles', () => {
    expect(nativeFinanceLane4Configured()).toBe(true);
    expect(Object.keys(SQL_ORACLE).sort()).toEqual(['accounts', 'charts', 'parties']);
    for (const sql of Object.values(SQL_ORACLE)) {
      expect(sql).toMatch(/^SELECT /);
      expect(sql).not.toMatch(/INSERT|UPDATE|DELETE/);
      expect(sql).toContain('company_id=?');
      expect(sha256(sql)).toMatch(/^[0-9a-f]{64}$/);
    }
    expect(sha256(SQL_ORACLE.accounts)).toBe('7acdf63122f6310dbf047a2b45c42b14c5fe40c018300d5761e851801aa71dcc');
    expect(sha256(SQL_ORACLE.charts)).toBe('6ce4d387ffc287dd1f9f87b5864e138ebf2da6c085fc35d10cffacd17694a5db');
    expect(sha256(SQL_ORACLE.parties)).toBe('532180fd2b13a268349acafc704a456d9fbaf178bc36cfcc19fbc0e81f6d5e78');
    expect(
      sha256(JSON.stringify([{ category: 'asset', chart_id: 'chart-1', code: '100', id: 'acct-1', name: 'Cash' }])),
    ).toBe('603f1efb79efc13158e69910dba1850f76377aafec03ee83b5fc8808be23fa13');
  });

  it('selects every registered Holding by WorkContext with exact host/slug isolation', async () => {
    const calls = stubFetch();
    for (const i of [0, 1, 2] as const) {
      expect(await financeReferenceOptions('accounts', 'acme-co', actor(i))).toEqual(ROWS.accounts);
      expect(await financeReferenceOptions('charts', 'acme-co', actor(i))).toEqual(ROWS.charts);
      expect(await financeReferenceOptions('parties', 'acme-co', actor(i))).toEqual(ROWS.parties);
    }
    expect(calls).toHaveLength(9);
    for (const [index, call] of calls.entries()) {
      const i = Math.floor(index / 3) as 0 | 1 | 2;
      expect(call.url).toBe(`http://127.0.0.1:3198/api/v1/holdings/${HOLDINGS[i].holdingSlug}/finance/native-lane-4/reference-options`);
      expect(call.headers.get('host')).toBe(HOLDINGS[i].host);
      expect(call.headers.get('origin')).toBe(HOLDINGS[i].origin);
      expect(call.headers.get('x-erpx-context-company')).toBe('acme-co');
      expect(call.headers.get('x-erpx-lane4-kind')).toBe(['accounts', 'charts', 'parties'][index % 3]);
    }
  });

  it('fails closed on unknown, missing or contradictory WorkContext identity', async () => {
    stubFetch();
    await expect(financeReferenceOptions('charts', 'acme-co', { holdingId: 'h404', holdingSlug: 'acme' }))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    await expect(financeReferenceOptions('charts', 'acme-co', { holdingId: 'h001', holdingSlug: 'northstar' }))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    await expect(financeReferenceOptions('charts', 'acme-co', {}))
      .rejects.toMatchObject({ code: 'NATIVE_FINANCE_LANE4_REQUEST_INVALID', status: 422 });
    // Configured companyId binds acme only; a different company fails closed.
    await expect(financeReferenceOptions('charts', 'other-co', actor(0)))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    expect(vi.mocked(fetch).mock.calls.length).toBe(0);
  });

  it('rejects malformed configurations, including a first-holding fallback', async () => {
    const invalid = [
      { ...config, holdings: [] },
      { ...config, holdings: Array.from({ length: 65 }, (_, i) => HOLDINGS[i % 3]) },
      { ...config, holdings: [HOLDINGS[0], { ...HOLDINGS[0], holdingSlug: 'again' }] },
      { ...config, holdings: [{ ...HOLDINGS[0], host: 'other.example.com' }] },
      { ...config, holdings: [HOLDINGS[0]].map((h) => ({ ...h, host: undefined })) },
    ];
    for (const value of invalid) {
      process.env[ENV] = JSON.stringify(value);
      await expect(financeReferenceOptions('charts', 'acme-co', actor(0)))
        .rejects.toMatchObject({ code: 'NATIVE_FINANCE_LANE4_UNAVAILABLE', status: 503 });
    }
    process.env[ENV] = JSON.stringify(config);
    stubFetch();
    expect(await financeReferenceOptions('charts', 'acme-co', actor(1))).toEqual(ROWS.charts);
  });

  it('fails closed when the native service is unavailable or the reply is malformed', async () => {
    delete process.env[ENV];
    await expect(financeReferenceOptions('charts', 'acme-co', actor(0))).rejects.toMatchObject({
      code: 'NATIVE_FINANCE_LANE4_UNAVAILABLE', status: 503,
    });
    process.env[ENV] = JSON.stringify(config);

    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('down'); }));
    await expect(financeReferenceOptions('charts', 'acme-co', actor(0))).rejects.toMatchObject({
      code: 'NATIVE_FINANCE_LANE4_UNAVAILABLE', status: 503,
    });

    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ data: [{ id: 'x' }] }), {
      status: 200, headers: { 'content-type': 'application/json' },
    })));
    await expect(financeReferenceOptions('charts', 'acme-co', actor(0))).rejects.toMatchObject({
      code: 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID', status: 502,
    });

    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: { code: 'FORBIDDEN' } }), {
      status: 403, headers: { 'content-type': 'application/json' },
    })));
    await expect(financeReferenceOptions('charts', 'acme-co', actor(0))).rejects.toMatchObject({ status: 403 });
  });

  it('keeps the PMS consumer as SQL-free transport over the native client', async () => {
    const calls = stubFetch();
    const prepared: unknown[][] = [];
    const db = {
      prepare: (sql: string) => {
        prepared.push([sql]);
        return { bind: () => ({ all: async () => ({ results: [] }) }) };
      },
    };
    const facts = pmsFinanceFacts(db as never, actor(2));
    expect(await facts.readFinanceAccounts({ companyId: 'acme-co' })).toEqual(ROWS.accounts);
    expect(await facts.readFinanceCharts({ companyId: 'acme-co' })).toEqual(ROWS.charts);
    expect(await facts.readFinanceParties({ companyId: 'acme-co' })).toEqual(ROWS.parties);
    expect(prepared).toEqual([]);
    expect(calls.map((call) => call.headers.get('host'))).toEqual([
      'erpx-internal.jesadakorn.com', 'erpx-internal.jesadakorn.com', 'erpx-internal.jesadakorn.com',
    ]);
  });
});
