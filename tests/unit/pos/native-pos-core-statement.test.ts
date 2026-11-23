import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePosShiftsStatement,
  nativePosTablesStatement,
  nativePosCashMovementsStatement,
  nativePosSalesEvidenceStatement,
  nativePosSaleByIdStatement,
  nativePosSettingsCurrencyStatement,
} from '../../../packages/server-pos/src/adapters/native-pos-core-statement';
import { pmsHospitalitySetupPosFacts } from '../../../packages/server-pos/src/adapters/pms-hospitality-setup-facts';

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
});
