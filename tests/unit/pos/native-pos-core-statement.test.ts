import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePosShiftsStatement,
  nativePosTablesStatement,
  nativePosCashMovementsStatement,
  nativePosSalesEvidenceStatement,
  nativePosSaleByIdStatement,
  nativePosSettingsCurrencyStatement,
  nativePosPostingJournalStatement,
  nativePosRegisterCatalogStatement,
  nativePosSettingsAccountsStatement,
  nativePosConnectionsStatement,
  nativePosPaymentByIdStatement,
  nativePosBookingConnectionStatement,
  nativePosStripeEntrypointJobsStatement,
  nativePosMovementSaleStatement,
  nativePosCatalogStatement,
  nativePosProductImagesStatement,
  nativePosPromptPayConnectionStatement,
  nativePosChargeConnectionStatement,
  nativePosConnectionAccountStatement,
  nativePosCounterPaymentConnectionStatement,
} from '../../../packages/server-pos/src/adapters/native-pos-core-statement';
import { pmsHospitalitySetupPosFacts } from '../../../packages/server-pos/src/adapters/pms-hospitality-setup-facts';
import { pmsPosFacts } from '../../../packages/server-pos/src/adapters/pms-command-facts';
import { pmsBookingChangeFacts } from '../../../packages/server-pos/src/adapters/pms-booking-change-facts';
import { pmsBookingPosFacts } from '../../../packages/server-pos/src/adapters/pms-booking-facts';
import { pmsCounterPaymentFacts } from '../../../packages/server-pos/src/adapters/pms-counter-payment-facts';
import { stripeEntrypointFacts } from '../../../packages/server-pos/src/adapters/stripe-entrypoint-facts';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
const scope = { company: 'company', branch: 'branch', branch2: 'branch' };

afterEach(() => {
  vi.unstubAllGlobals();
  if (previous === undefined) delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
  else process.env.ERPX_NATIVE_POS_OWNER_CONFIG = previous;
});

function mockDb(seen: { sql?: string; values?: unknown[] }, rows: unknown[] = []) {
  return {
    prepare: (sql: string) => ({
      bind: (...values: unknown[]) => {
        seen.sql = sql;
        seen.values = values;
        return { all: async () => ({ results: rows, success: true, meta: { changes: 0, duration: 0 } }) };
      },
    }),
  };
}

function stubFetch(sql: string) {
  const fetch = vi.fn().mockResolvedValue(
    new Response(JSON.stringify({ data: { sql, values: ['company', 'branch', 'branch'] } }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    }),
  );
  vi.stubGlobal('fetch', fetch);
  return fetch;
}

describe('native POS core workspace read bridge (shifts family)', () => {
  it.each([
    ['pos_core_shifts', nativePosShiftsStatement, 'SELECT s.*,r.name register_name FROM pos_shifts s'],
    ['pos_core_tables', nativePosTablesStatement, 'SELECT t.* FROM pos_tables t'],
    ['pos_core_cash_movements', nativePosCashMovementsStatement, 'SELECT m.*,s.register_id FROM pos_cash_movements m'],
  ] as const)('requests the owner kind %s with Company/branch scope only', async (kind, bridge, sql) => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const fetch = stubFetch(sql);
    const seen: { sql?: string; values?: unknown[] } = {};
    const rows = [{ id: 'row' }];
    const result = await bridge(mockDb(seen, rows) as never, scope);
    expect(result).toEqual(rows);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('company');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind, values: ['company', 'branch', 'branch'] });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(['company', 'branch', 'branch']);
  });

  it('fails closed when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosShiftsStatement(mockDb({}) as never, scope)).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('serves the hospitality-setup tables read from the native registry, not local SQL', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT t.* FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id';
    const fetch = stubFetch(sql);
    const seen: { sql?: string; values?: unknown[] } = {};
    const db = mockDb(seen, [{ id: 'table' }]);
    const prepare = vi.spyOn(db, 'prepare');
    const rows = await pmsHospitalitySetupPosFacts(db as never).tables({ company: 'company', branch: 'branch' });
    expect(rows).toEqual([{ id: 'table' }]);
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({
      company: 'company',
      kind: 'pos_core_tables',
      values: ['company', 'branch', 'branch'],
    });
    // The only prepared SQL is the registry's authoritative statement returned
    // by the native service; no inline pos_tables literal runs locally.
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(seen.sql).toBe(sql);
    expect(seen.sql).not.toContain('r.branch_id=?');
  });

  it('requests the executive-evidence sale row from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = "SELECT s.id source_id,s.id sale_id,'sale' source_type,s.number receipt_number,s.business_date FROM pos_sales s WHERE s.id=? AND s.company_id=?";
    const values = ['sale', 'company', 'branch', 'branch', 'THB', '2026-01-01', '2026-01-31'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { sale_id: 'sale', amount_minor: '100' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const facts = {
      id: 'sale', company: 'company', branch: 'branch', branch2: 'branch',
      currency: 'THB', from: '2026-01-01', to: '2026-01-31',
    };
    const result = await nativePosSalesEvidenceStatement(db as never, facts);
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({
      company: 'company',
      kind: 'core_readpossales3_520',
      values,
    });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the evidence read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    const facts = {
      id: 'sale', company: 'company', branch: null, branch2: null,
      currency: 'THB', from: '2026-01-01', to: '2026-01-31',
    };
    await expect(nativePosSalesEvidenceStatement({} as never, facts)).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('requests the by-id sale row from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT * FROM pos_sales WHERE id=?';
    const values = ['sale-1'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { id: 'sale-1', number: 'S-1', state: 'completed' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const result = await nativePosSaleByIdStatement(db as never, 'sale-1');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({
      company: '',
      kind: 'core_readpossalesbyid_521',
      values,
    });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the by-id sale read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosSaleByIdStatement({} as never, 'sale-1')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('requests the settings-currency row from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT currency FROM pos_settings WHERE company_id=?';
    const values = ['company'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { currency: 'THB' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const result = await nativePosSettingsCurrencyStatement(db as never, 'company');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({
      company: 'company',
      kind: 'core_readpossettingscurrency_522',
      values,
    });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the settings-currency read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosSettingsCurrencyStatement({} as never, 'company')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('requests the posting-journal id from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT journal_id FROM pos_postings WHERE source_key=?';
    const values = ['posting-1'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { journal_id: 'journal-1' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const result = await nativePosPostingJournalStatement(db as never, 'posting-1');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({
      company: '',
      kind: 'core_readpospostingsjournal_523',
      values,
    });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the posting-journal read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosPostingJournalStatement({} as never, 'posting-1')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('requests the register-catalog membership row from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=? AND r.branch_id=? AND r.company_id=?';
    const values = ['product-1', 'branch-1', 'company'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { product_id: 'product-1' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const result = await nativePosRegisterCatalogStatement(db as never, 'product-1', 'branch-1', 'company');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(JSON.parse(init.body)).toEqual({
      company: 'company',
      kind: 'core_readposregistercatalog_524',
      values,
    });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the register-catalog read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosRegisterCatalogStatement({} as never, 'product-1', 'branch-1', 'company')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('requests the settings accounts row from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT accounts_json FROM pos_settings WHERE company_id=?';
    const values = ['company'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { accounts_json: '{"bank":"bank-1"}' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const result = await nativePosSettingsAccountsStatement(db as never, 'company');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(JSON.parse(init.body)).toEqual({
      company: 'company',
      kind: 'core_readpossettingsaccounts_525',
      values,
    });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the settings accounts read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosSettingsAccountsStatement({} as never, 'company')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('requests the by-company connection row from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT * FROM pos_connections WHERE company_id=?';
    const values = ['company'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { company_id: 'company', account_id: 'acct_1', state: 'ready' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const result = await nativePosConnectionsStatement(db as never, 'company');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(JSON.parse(init.body)).toEqual({
      company: 'company',
      kind: 'stripe_readposconnections_526',
      values,
    });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the connections read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosConnectionsStatement({} as never, 'company')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it.each([
    ['booking-change', () => pmsBookingChangeFacts, 'readPosSettings'],
    ['booking', () => pmsBookingPosFacts, 'readPosSettings1'],
    ['counter-payment', () => pmsCounterPaymentFacts, 'readPosSettings'],
  ] as const)('serves the PMS %s accounts read from the native registry, not local SQL', async (_name, factory, method) => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT accounts_json FROM pos_settings WHERE company_id=?';
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values: ['company'] } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { accounts_json: '{"bank":"bank-1"}' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const prepare = vi.spyOn(db, 'prepare');
    // Only the accounts read is exercised here; the sole prepared SQL is the
    // registry statement returned by the native service, never a local literal.
    const facts = (factory()(db as never) as Record<string, (arg: { companyId: string }) => Promise<unknown>>)[method];
    await expect(facts({ companyId: 'company' })).resolves.toEqual(row);
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({
      company: 'company',
      kind: 'core_readpossettingsaccounts_525',
      values: ['company'],
    });
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(['company']);
  });

  it('requests the Stripe by-id payment row from the native registry with the FOR UPDATE lock', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE';
    const values = ['payment-1'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { id: 'payment-1', state: 'pending' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const prepare = vi.spyOn(db, 'prepare');
    const result = await nativePosPaymentByIdStatement(db as never, 'payment-1');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({
      company: '',
      kind: 'stripe_readpospaymentsbyid_527',
      values,
    });
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(seen.sql).toBe(sql);
    expect(seen.sql).toContain('FOR UPDATE');
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the by-id payment read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosPaymentByIdStatement({} as never, 'payment-1')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps all four Stripe by-id payment reads on the native registry with no local SQL', async () => {
    const { readFileSync } = await import('node:fs');
    const source = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-stripe.ts', import.meta.url), 'utf8');
    expect(source.match(/nativePosPaymentByIdStatement<PosRow>\(db, facts\.id\)/g)).toHaveLength(4);
    expect(source).not.toContain('SELECT * FROM pos_payments WHERE id=? FOR UPDATE');
  });

  it('requests the PMS booking connection row from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = "SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true";
    const values = ['company'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { account_id: 'acct_1', livemode: false, card_enabled: true, promptpay_enabled: false };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const prepare = vi.spyOn(db, 'prepare');
    const result = await nativePosBookingConnectionStatement(db as never, 'company');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body)).toEqual({
      company: 'company',
      kind: 'stripe_readposbookingconnection_528',
      values,
    });
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the booking connection read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosBookingConnectionStatement({} as never, 'company')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('serves both PMS booking connection reads from the native registry with no local SQL', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = "SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true";
    const fetch = vi.fn().mockImplementation(() => Promise.resolve(
      new Response(JSON.stringify({ data: { sql, values: ['company'] } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    ));
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { account_id: 'acct_1', livemode: false, card_enabled: true, promptpay_enabled: false };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const prepare = vi.spyOn(db, 'prepare');
    await expect(pmsBookingChangeFacts(db as never).readPosConnections({ companyId: 'company' })).resolves.toEqual(row);
    await expect(pmsBookingPosFacts(db as never).readPosConnections1({ companyId: 'company' })).resolves.toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(2);
    for (const call of fetch.mock.calls)
      expect(JSON.parse(call[1].body)).toEqual({
        company: 'company',
        kind: 'stripe_readposbookingconnection_528',
        values: ['company'],
      });
    expect(prepare).toHaveBeenCalledTimes(2);
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(['company']);
  });

  it('keeps no byte-identical PMS booking connection SQL literal in the TS adapters', async () => {
    const { readFileSync } = await import('node:fs');
    const literal = "SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true";
    for (const file of ['pms-booking-change-facts.ts', 'pms-booking-facts.ts']) {
      const source = readFileSync(new URL(`../../../packages/server-pos/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(source).not.toContain(literal);
      expect(source).toContain('nativePosBookingConnectionStatement');
    }
  });

  it('requests the Stripe entrypoint jobs rows from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = "SELECT id,kind,state,last_error,payload_json FROM pos_jobs WHERE company_id=? AND kind IN ('connect','connect_refresh') ORDER BY created_at DESC LIMIT 10";
    const values = ['company'];
    const rows = [{ id: 'job', kind: 'connect', state: 'completed', last_error: null, payload_json: '{}' }];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { all: async () => ({ results: rows, success: true, meta: { changes: 0, duration: 0 } }) };
        },
      }),
    };
    const prepare = vi.spyOn(db, 'prepare');
    const result = await nativePosStripeEntrypointJobsStatement(db as never, 'company');
    expect(result.results).toEqual(rows);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind: 'stripe_readposjobsentrypoint_529', values });
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the Stripe entrypoint jobs read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosStripeEntrypointJobsStatement({} as never, 'company')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('serves both Stripe entrypoint reads from the native registry with no local SQL', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const connectionSql = 'SELECT * FROM pos_connections WHERE company_id=?';
    const jobsSql = "SELECT id,kind,state,last_error,payload_json FROM pos_jobs WHERE company_id=? AND kind IN ('connect','connect_refresh') ORDER BY created_at DESC LIMIT 10";
    const connectionRow = { company_id: 'company', account_id: 'acct_1', state: 'ready' };
    const jobRows = [{ id: 'job', kind: 'connect', state: 'completed', last_error: null, payload_json: '{}' }];
    const fetch = vi.fn().mockImplementation((...args: unknown[]) => {
      const kind = String(JSON.parse(String((args[1] as { body: string }).body)).kind);
      const data = kind === 'stripe_readposjobsentrypoint_529'
        ? { sql: jobsSql, values: ['company'] }
        : { sql: connectionSql, values: ['company'] };
      return Promise.resolve(new Response(JSON.stringify({ data }), { status: 200, headers: { 'content-type': 'application/json' } }));
    });
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] }[] = [];
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.push({ sql: text, values: bound });
          return {
            first: async () => connectionRow,
            all: async () => ({ results: jobRows, success: true, meta: { changes: 0, duration: 0 } }),
          };
        },
      }),
    };
    const facts = stripeEntrypointFacts(db as never);
    await expect(facts.connection('company')).resolves.toEqual(connectionRow);
    const jobs = await facts.jobs('company');
    expect(jobs.results).toEqual(jobRows);
    expect(fetch).toHaveBeenCalledTimes(2);
    const kinds = fetch.mock.calls.map((call: unknown[]) => String(JSON.parse(String((call[1] as { body: string }).body)).kind));
    expect(kinds).toEqual(['stripe_readposconnections_526', 'stripe_readposjobsentrypoint_529']);
    expect(seen.map(entry => entry.sql)).toEqual([connectionSql, jobsSql]);
    expect(seen.map(entry => entry.values)).toEqual([['company'], ['company']]);
  });

  it('keeps no byte-identical Stripe entrypoint SQL literal in the TS adapter', async () => {
    const { readFileSync } = await import('node:fs');
    const source = readFileSync(new URL('../../../packages/server-pos/src/adapters/stripe-entrypoint-facts.ts', import.meta.url), 'utf8');
    expect(source).not.toContain('SELECT * FROM pos_connections WHERE company_id=?');
    expect(source).not.toContain('SELECT id,kind,state,last_error,payload_json FROM pos_jobs');
    expect(source).toContain('nativePosConnectionsStatement');
    expect(source).toContain('nativePosStripeEntrypointJobsStatement');
  });

  it('requests the presentation movement-sale projection from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT l.sale_id FROM pos_stock_moves m JOIN pos_lines l ON l.fulfillment_id=m.fulfillment_id JOIN pos_sales s ON s.id=l.sale_id WHERE m.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.created_at LIMIT 1';
    const values = ['movement', 'company', null, null];
    const rows = [{ sale_id: 'sale' }];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => rows[0] };
        },
      }),
    };
    const prepare = vi.spyOn(db, 'prepare');
    const result = await nativePosMovementSaleStatement(db as never, 'movement', 'company', null);
    expect(result).toEqual(rows[0]);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind: 'pos_readposmovementsale_530', values });
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the movement-sale read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosMovementSaleStatement({} as never, 'movement', 'company', null)).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps no byte-identical movement-sale SQL literal in the TS adapter', async () => {
    const { readFileSync } = await import('node:fs');
    const source = readFileSync(new URL('../../../packages/server-pos/src/adapters/movement-presentation-query.ts', import.meta.url), 'utf8');
    expect(source).not.toContain('SELECT l.sale_id FROM pos_stock_moves');
    expect(source).toContain('nativePosMovementSaleStatement');
  });

  it('requests the product-image catalog guard row from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT product_id,version FROM pos_catalog WHERE product_id=? AND company_id=?';
    const values = ['product-1', 'company'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { product_id: 'product-1', version: 3 };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const prepare = vi.spyOn(db, 'prepare');
    const result = await nativePosCatalogStatement(db as never, 'product-1', 'company');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind: 'images_readposcatalog_531', values });
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the product-image catalog guard read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosCatalogStatement({} as never, 'product-1', 'company')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('requests the product-image attachment-source guard row from the native owner registry only', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT attachment_source_id FROM pos_product_images WHERE id=? AND company_id=? AND product_id=?';
    const values = ['image-1', 'company', 'product-1'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { attachment_source_id: 'source-1' };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const prepare = vi.spyOn(db, 'prepare');
    const result = await nativePosProductImagesStatement(db as never, 'image-1', 'company', 'product-1');
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind: 'images_readposproductimages_532', values });
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
  });

  it('fails closed for the product-image attachment-source guard read when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosProductImagesStatement({} as never, 'image-1', 'company', 'product-1')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps no byte-identical product-image guard SQL literal in the TS adapter', async () => {
    const { readFileSync } = await import('node:fs');
    const source = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-images.ts', import.meta.url), 'utf8');
    expect(source).not.toContain('SELECT product_id,version FROM pos_catalog');
    expect(source).not.toContain('SELECT attachment_source_id FROM pos_product_images');
    expect(source).toContain('nativePosCatalogStatement');
    expect(source).toContain('nativePosProductImagesStatement');
  });
});

describe('native POS PMS/counter connection read bridge (SELECT-only family)', () => {
  const cases = [
    {
      name: 'PromptPay/daily-payout probe',
      kind: 'pms_readposconnectionspromptpay_533',
      sql: "SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'",
      values: ['company'],
      bridge: nativePosPromptPayConnectionStatement,
      args: ['company'] as const,
    },
    {
      name: 'card/PromptPay charge-readiness probe',
      kind: 'pms_readposconnectionscharge_534',
      sql: "SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true) AND (livemode=false OR ?='true')",
      values: ['company', 'true'],
      bridge: nativePosChargeConnectionStatement,
      args: ['company', 'true'] as const,
    },
    {
      name: 'account/livemode probe',
      kind: 'pms_readposconnectionsaccount_535',
      sql: "SELECT account_id,livemode FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true)",
      values: ['company'],
      bridge: nativePosConnectionAccountStatement,
      args: ['company'] as const,
    },
    {
      name: 'counter-payment readiness read',
      kind: 'counter_readposconnectionreadiness_536',
      sql: "SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'",
      values: ['company'],
      bridge: nativePosCounterPaymentConnectionStatement,
      args: ['company'] as const,
    },
  ] as const;

  it.each(cases)('requests the $name from the native owner registry only', async ({ kind, sql, values, bridge, args }) => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const row = { account_id: 'acct_1', livemode: false };
    const db = {
      prepare: (text: string) => ({
        bind: (...bound: unknown[]) => {
          seen.sql = text;
          seen.values = bound;
          return { first: async () => row };
        },
      }),
    };
    const prepare = vi.spyOn(db, 'prepare');
    const result = await (bridge as (db: never, ...rest: string[]) => Promise<unknown>)(db as never, ...args);
    expect(result).toEqual(row);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('company');
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind, values });
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
    expect(seen.sql).not.toMatch(/FOR UPDATE|FOR SHARE/);
  });

  it.each(cases)('fails closed for $name when the native owner service is unconfigured', async ({ bridge, args }) => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect((bridge as (db: never, ...rest: string[]) => Promise<unknown>)({} as never, ...args)).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('serves all three PMS command connection probes from the native registry', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const byKind: Record<string, { sql: string; values: unknown[] }> = {
      pms_readposconnectionspromptpay_533: { sql: "SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'", values: ['company'] },
      pms_readposconnectionscharge_534: { sql: "SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true) AND (livemode=false OR ?='true')", values: ['company', 'false'] },
      pms_readposconnectionsaccount_535: { sql: "SELECT account_id,livemode FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true)", values: ['company'] },
    };
    const fetch = vi.fn().mockImplementation((_url: string, init: { body: string }) => {
      const { kind } = JSON.parse(init.body) as { kind: string };
      const data = byKind[kind];
      return Promise.resolve(
        new Response(JSON.stringify({ data }), { status: 200, headers: { 'content-type': 'application/json' } }),
      );
    });
    vi.stubGlobal('fetch', fetch);
    const seen: string[] = [];
    const db = {
      prepare: (text: string) => ({
        bind: () => {
          seen.push(text);
          return { first: async () => ({ account_id: 'acct_1', livemode: false }) };
        },
      }),
    };
    const facts = pmsPosFacts(db as never);
    await facts.readPosConnections({ companyId: 'company' });
    await facts.readPosConnections2({ companyId: 'company', fact1: 'false' });
    await facts.readPosConnections3({ companyId: 'company' });
    expect(fetch).toHaveBeenCalledTimes(3);
    expect(seen).toEqual([
      byKind.pms_readposconnectionspromptpay_533.sql,
      byKind.pms_readposconnectionscharge_534.sql,
      byKind.pms_readposconnectionsaccount_535.sql,
    ]);
  });

  it('keeps no byte-identical PMS/counter connection SQL literal in the TS adapters', async () => {
    const { readFileSync } = await import('node:fs');
    const command = readFileSync(new URL('../../../packages/server-pos/src/adapters/pms-command-facts.ts', import.meta.url), 'utf8');
    const counter = readFileSync(new URL('../../../packages/server-pos/src/adapters/pms-counter-payment-facts.ts', import.meta.url), 'utf8');
    for (const source of [command, counter]) {
      expect(source).not.toContain('FROM pos_connections');
      expect(source).not.toMatch(/FOR UPDATE|FOR SHARE/);
    }
    expect(command).toContain('nativePosPromptPayConnectionStatement');
    expect(command).toContain('nativePosChargeConnectionStatement');
    expect(command).toContain('nativePosConnectionAccountStatement');
    expect(counter).toContain('nativePosCounterPaymentConnectionStatement');
  });
});
