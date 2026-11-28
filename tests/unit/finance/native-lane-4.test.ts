import { createHash } from 'node:crypto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { financePostingGuard, financeReferenceOptions, nativeFinanceLane4Configured, type Lane4Actor } from '../../../modules/finance/server/src/adapters/native-lane-4/client';
import { pmsFinanceFacts } from '../../../modules/finance/server/src/public/pms-command-facts';

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

/** Lock-free posting guard renderings retired from `pms-command-facts.ts`.
 * The liability one is rendered by `ACCOUNT_SELECT_PREFIX` in the shared
 * `finance-read-tail-r30-sql.ts` module. */
const GUARD_SQL_ORACLE = {
  chart_active: "SELECT id FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency=?",
  account_liability: "SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category='liability' AND status='active'",
  party_customer: "SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND currency=? AND person_id IS NULL",
} as const;

const GUARD_ROW = { id: 'probe-1' };

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
    if (new URL(url.toString()).pathname.endsWith('/posting-guard'))
      return new Response(JSON.stringify({ data: GUARD_ROW }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
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

  it('pins the lock-free posting guard SQL oracles', () => {
    for (const sql of Object.values(GUARD_SQL_ORACLE)) {
      expect(sql).toMatch(/^(SELECT id|SELECT 1) FROM finance_/);
      expect(sql).toContain('company_id=?');
      expect(sql).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
      expect(sha256(sql)).toMatch(/^[0-9a-f]{64}$/);
    }
    expect(sha256(GUARD_SQL_ORACLE.chart_active)).toBe('e0d6087cb00a7d070006fbe0e3d60af1a171f69dc789c3377a6769f2b204f50e');
    expect(sha256(GUARD_SQL_ORACLE.account_liability)).toBe('58f166cb8e98ac1f60a648e149316ea01e6910645b3af0e4cd80f33bb5c6a061');
    expect(sha256(GUARD_SQL_ORACLE.party_customer)).toBe('7d208210d860c9156ed05c89e3ff05384688efddfece5419eb0cb54ca93e046e');
  });

  it('runs each posting guard on the caller Holding with exact binds', async () => {
    const calls = stubFetch();
    for (const i of [0, 1, 2] as const) {
      expect(await financePostingGuard('chart_active', 'acme-co', actor(i), { id: 'chart-1', currency: 'THB' })).toEqual(GUARD_ROW);
      expect(await financePostingGuard('account_liability', 'acme-co', actor(i), { id: 'acct-1', chartId: 'chart-1', currency: 'THB' })).toEqual(GUARD_ROW);
      expect(await financePostingGuard('party_customer', 'acme-co', actor(i), { id: 'party-1', currency: 'THB' })).toEqual(GUARD_ROW);
    }
    expect(calls).toHaveLength(9);
    for (const [index, call] of calls.entries()) {
      const i = Math.floor(index / 3) as 0 | 1 | 2;
      expect(call.url).toBe(`http://127.0.0.1:3198/api/v1/holdings/${HOLDINGS[i].holdingSlug}/finance/native-lane-4/posting-guard`);
      expect(call.headers.get('host')).toBe(HOLDINGS[i].host);
      expect(call.headers.get('x-erpx-context-company')).toBe('acme-co');
      expect(call.headers.get('x-erpx-lane4-id')).toBe(['chart-1', 'acct-1', 'party-1'][index % 3]);
      expect(call.headers.get('x-erpx-lane4-currency')).toBe('THB');
      expect(call.headers.get('x-erpx-lane4-kind')).toBe(['chart_active', 'account_liability', 'party_customer'][index % 3]);
      expect(call.headers.get('x-erpx-lane4-chart')).toBe(index % 3 === 1 ? 'chart-1' : null);
    }
  });

  it('fails closed on malformed guard input, unknown targets and invalid replies', async () => {
    stubFetch();
    await expect(financePostingGuard('chart_active', 'acme-co', { holdingId: 'h404', holdingSlug: 'acme' }, { id: 'chart-1', currency: 'THB' }))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    await expect(financePostingGuard('account_liability', 'acme-co', actor(0), { id: 'acct-1', currency: 'THB' }))
      .rejects.toMatchObject({ code: 'NATIVE_FINANCE_LANE4_REQUEST_INVALID', status: 422 });
    await expect(financePostingGuard('chart_active', 'acme-co', actor(0), { id: 'chart-1', currency: 'bad-currency' }))
      .rejects.toMatchObject({ code: 'NATIVE_FINANCE_LANE4_REQUEST_INVALID', status: 422 });

    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ data: { id: 'x', extra: 1 } }), {
      status: 200, headers: { 'content-type': 'application/json' },
    })));
    await expect(financePostingGuard('chart_active', 'acme-co', actor(0), { id: 'chart-1', currency: 'THB' }))
      .rejects.toMatchObject({ code: 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID', status: 502 });

    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ data: null }), {
      status: 200, headers: { 'content-type': 'application/json' },
    })));
    expect(await financePostingGuard('chart_active', 'acme-co', actor(0), { id: 'chart-1', currency: 'THB' })).toBeNull();
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
    expect(await facts.readFinanceCharts2({ chartId: 'chart-1', companyId: 'acme-co', currency: 'THB' })).toEqual(GUARD_ROW);
    expect(await facts.readFinanceAccounts4({ id: 'acct-1', companyId: 'acme-co', chartId: 'chart-1', currency: 'THB' })).toEqual(GUARD_ROW);
    expect(await facts.readFinanceParties2({ partyId: 'party-1', companyId: 'acme-co', currency: 'THB' })).toEqual(GUARD_ROW);
    expect(prepared).toEqual([]);
    expect(calls.map((call) => call.headers.get('host'))).toEqual([
      'erpx-internal.jesadakorn.com', 'erpx-internal.jesadakorn.com', 'erpx-internal.jesadakorn.com',
      'erpx-internal.jesadakorn.com', 'erpx-internal.jesadakorn.com', 'erpx-internal.jesadakorn.com',
    ]);
    expect(calls.filter((call) => call.url.includes('posting-guard'))).toHaveLength(3);
  });
});
