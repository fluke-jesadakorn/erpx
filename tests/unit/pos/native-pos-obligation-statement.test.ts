import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePosAddonObligationsStatement,
  nativePosPackageObligationsStatement,
  nativePosPmsPackageObligationsStatement,
} from '../../../packages/server-pos/src/adapters/native-pos-obligation-statement';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;

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

function stubFetch(sql: string, values: unknown[]) {
  const fetch = vi.fn().mockResolvedValue(
    new Response(JSON.stringify({ data: { sql, values } }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    }),
  );
  vi.stubGlobal('fetch', fetch);
  return fetch;
}

describe('native POS obligation read bridge', () => {
  it.each([
    ['pos_addon_obligations_restaurant', ['company', 'pos-restaurant'], (db: never) => nativePosAddonObligationsStatement(db, 'company', 'pos-restaurant')],
    ['pos_addon_obligations_hotel', ['company', 'pos-hotel-charge'], (db: never) => nativePosAddonObligationsStatement(db, 'company', 'pos-hotel-charge')],
    ['pos_package_obligations', ['company'], (db: never) => nativePosPackageObligationsStatement(db, 'company')],
    ['pms_pos_package_obligations', ['company'], (db: never) => nativePosPmsPackageObligationsStatement(db, 'company')],
  ] as const)('requests owner kind %s with scoped values only', async (kind, values, bridge) => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'WITH scope AS (SELECT ?::text company_id) SELECT 1';
    const fetch = stubFetch(sql, [...values]);
    const seen: { sql?: string; values?: unknown[] } = {};
    const rows = [{ id: 'row' }];
    const result = await bridge(mockDb(seen, rows) as never);
    expect(result).toEqual(rows);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('company');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind, values: [...values] });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual([...values]);
  });

  it('fails closed on an unregistered addon package without contacting the owner', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosAddonObligationsStatement(mockDb({}) as never, 'company', 'pos')).rejects.toThrow(
      'POS_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED',
    );
    expect(fetch).not.toHaveBeenCalled();
  });

  it('fails closed when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosPackageObligationsStatement(mockDb({}) as never, 'company')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });
});
