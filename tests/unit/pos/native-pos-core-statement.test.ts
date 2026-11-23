import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePosShiftsStatement,
  nativePosTablesStatement,
  nativePosCashMovementsStatement,
} from '../../../packages/server-pos/src/adapters/native-pos-core-statement';

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
});
