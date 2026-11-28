import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativePosRegisterPropertyStatement,
  nativePosFulfillmentPropertyStatement,
} from '../../../modules/pos/server/src/adapters/native-pos-hotel-reads';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44997', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
const rows = [{ property_id: 'prop', branch_id: 'br' }];
const cases = [
  {
    name: 'register property for a day assertion',
    kind: 'pos_hotel_register_property',
    sql: 'SELECT property_id,branch_id FROM pos_registers WHERE company_id=? AND id=?',
    values: ['co', 'reg'],
    bridge: nativePosRegisterPropertyStatement,
    args: ['co', 'reg'],
    list: false,
  },
  {
    name: 'fulfillment property rows for a day assertion',
    kind: 'pos_hotel_fulfillment_property',
    sql: 'SELECT DISTINCT r.property_id,r.branch_id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND l.fulfillment_id=?',
    values: ['co', 'ful'],
    bridge: nativePosFulfillmentPropertyStatement,
    args: ['co', 'ful'],
    list: true,
  },
] as const;

afterEach(() => {
  vi.unstubAllGlobals();
  if (previous === undefined) delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
  else process.env.ERPX_NATIVE_POS_OWNER_CONFIG = previous;
});

function mockDb(seen: { sql?: string; values?: unknown[] }) {
  return {
    prepare: (sql: string) => ({
      bind: (...values: unknown[]) => {
        seen.sql = sql;
        seen.values = values;
        return { first: async () => rows[0], all: async () => ({ results: rows }) };
      },
    }),
  };
}

describe('native POS hotel day-assertion property bridge (round 32)', () => {
  it.each(cases)('requests the $name from the native owner registry only', async (item) => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql: item.sql, values: [...item.values] } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const result = await (item.bridge as (db: never, ...args: string[]) => Promise<unknown>)(
      mockDb(seen) as never,
      ...item.args,
    );
    expect(result).toEqual(item.list ? rows : rows[0]);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0] as [string, RequestInit & { body: string; headers: Record<string, string> }];
    expect(url).toBe('http://127.0.0.1:44997/api/v1/companies/co/pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('co');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: 'co', kind: item.kind, values: [...item.values] });
    expect(seen.sql).toBe(item.sql);
    expect(seen.values).toEqual([...item.values]);
    expect(seen.sql).toMatch(/^SELECT /);
    expect(seen.sql!.match(/\?/g)).toHaveLength(item.values.length);
    expect(seen.sql).not.toMatch(/FOR UPDATE|FOR SHARE|FOR NO KEY UPDATE|LOCK TABLE|pg_advisory|INSERT|UPDATE |DELETE|RETURNING|pms_sources|pms_properties|pos_day_closes|pos_stripe_routes/);
  });

  it.each(cases)('fails closed for $name when the native owner service is unconfigured', async (item) => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(
      (item.bridge as (db: never, ...args: string[]) => Promise<unknown>)({} as never, ...item.args),
    ).rejects.toMatchObject({ code: 'NATIVE_POS_OWNER_UNAVAILABLE', status: 503 });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps no retired hotel property SQL literal in the TS adapters', () => {
    const core = readFileSync(new URL('../../../modules/pos/server/src/adapters/pos-core.ts', import.meta.url), 'utf8');
    for (const item of cases) expect(core).not.toContain(item.sql);
    expect(core.match(/nativePos(RegisterProperty|FulfillmentProperty)Statement\(db/g)).toHaveLength(2);
  });
});
