import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativePosAuditChargeToRoomStatement,
  nativePosAuditKitchenStartStatement,
} from '../../../packages/server-pos/src/adapters/native-pos-audit-reads';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
const rows = [{ actor_id: 'actor' }];

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
        return {
          all: async () => ({ results: rows, success: true, meta: { changes: 0, duration: 0 } }),
          first: async () => rows[0],
        };
      },
    }),
  };
}

const cases = [
  {
    name: 'actor that charged a sale to the room',
    kind: 'pos_audit_actor_charge_to_room',
    sql: "SELECT actor_id FROM pos_audit WHERE operation='sale.charge_to_room' AND record_id=? ORDER BY created_at DESC LIMIT 1",
    facts: { id: 'rec_1' },
    values: ['rec_1'],
    bridge: nativePosAuditChargeToRoomStatement,
  },
  {
    name: 'actor that started a kitchen fulfillment',
    kind: 'pos_audit_actor_kitchen_start',
    sql: "SELECT actor_id FROM pos_audit WHERE operation='kitchen.start' AND record_id=? ORDER BY created_at DESC LIMIT 1",
    facts: { id: 'rec_2' },
    values: ['rec_2'],
    bridge: nativePosAuditKitchenStartStatement,
  },
] as const;

describe('native POS audit actor read bridge', () => {
  it.each(cases)('requests the $name from the native owner registry only', async (item) => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql: item.sql, values: item.values } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const result = await (item.bridge as (db: never, facts: never) => Promise<unknown>)(
      mockDb(seen) as never, item.facts as never,
    );
    expect(result).toEqual(rows[0]);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies//pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: '', kind: item.kind, values: [...item.values] });
    expect(seen.sql).toBe(item.sql);
    expect(seen.values).toEqual([...item.values]);
    expect(seen.sql).toMatch(/^SELECT /);
    expect(seen.sql).not.toMatch(/FOR UPDATE|FOR SHARE|FOR NO KEY UPDATE|LOCK TABLE|pg_advisory|INSERT|UPDATE |DELETE|RETURNING|pms_sources|pms_properties/);
    expect(seen.sql!.match(/\?/g)?.length).toBe(item.values.length);
  });

  it.each(cases)('fails closed for $name when the native owner service is unconfigured', async (item) => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(
      (item.bridge as (db: never, facts: never) => Promise<unknown>)({} as never, item.facts as never),
    ).rejects.toMatchObject({ code: 'NATIVE_POS_OWNER_UNAVAILABLE', status: 503 });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps no retired audit actor SQL literal in the TS core adapter', () => {
    const core = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-core.ts', import.meta.url), 'utf8');
    for (const item of cases) expect(core).not.toContain(item.sql);
    for (const fn of ['nativePosAuditChargeToRoomStatement', 'nativePosAuditKitchenStartStatement'])
      expect(core).toContain(fn);
  });

  it('keeps row locks, the day-report registry and the Stripe registry inline', () => {
    const core = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-core.ts', import.meta.url), 'utf8');
    expect(core).toContain("SELECT * FROM pos_sales WHERE id=? FOR UPDATE");
    expect(core).toContain("SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE");
    const owner = readFileSync(new URL('../../../packages/server-pos/src/adapters/owner_writes.rs', import.meta.url), 'utf8');
    expect(owner).toContain('crate::adapters::day_report_reads::SETTLEMENTS');
    expect(owner).toContain('crate::adapters::stripe_reads::REFUND_ID_BY_PROVIDER');
    expect(owner).toContain('crate::adapters::audit_reads::AUDIT_ACTOR_CHARGE_TO_ROOM');
    expect(owner).toContain('crate::adapters::audit_reads::AUDIT_ACTOR_KITCHEN_START');
  });
});
