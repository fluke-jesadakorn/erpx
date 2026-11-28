import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativePosChildTailLinesSumStatement,
  nativePosChildTailCreatedByStatement,
  nativePosChildTailCompletedLineStatement,
  nativePosChildTailRefundPaymentStatement,
} from '../../../modules/pos/server/src/adapters/native-pos-child-tail';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44997', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
const rows = [{ id: 'row' }];
const facts = { fulfillment_id: 'ful-1', id: 'line-1', user: 'user-1' };

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
        return { first: async () => rows[0] };
      },
    }),
  };
}

const cases = [
  {
    name: 'fulfillment line quantity sum',
    kind: 'pos_child_tail_lines_sum',
    sql: 'SELECT COALESCE(SUM(quantity_milli),0) n FROM pos_lines WHERE fulfillment_id=? AND id<?',
    values: [facts.fulfillment_id, facts.id],
    bridge: nativePosChildTailLinesSumStatement,
  },
  {
    name: 'fulfillment line sale creator',
    kind: 'pos_child_tail_created_by',
    sql: 'SELECT s.created_by FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.created_by=? LIMIT 1',
    values: [facts.id, facts.user],
    bridge: nativePosChildTailCreatedByStatement,
  },
  {
    name: 'unreturned line of a completed sale',
    kind: 'pos_child_tail_completed_line',
    sql: "SELECT l.id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.state='completed' AND l.returned_milli<l.quantity_milli",
    values: [facts.id],
    bridge: nativePosChildTailCompletedLineStatement,
  },
  {
    name: 'unsettled refund payment for a sale',
    kind: 'pos_child_tail_refund_payment',
    sql: "SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE p.sale_id=? AND rp.state<>'succeeded'",
    values: ['sale-1'],
    bridge: nativePosChildTailRefundPaymentStatement,
  },
] as const;

type Bridge = (db: never, arg: never) => Promise<unknown>;
const invoke = (item: (typeof cases)[number], db: unknown) =>
  (item.bridge as Bridge)(
    db as never,
    (item.kind === 'pos_child_tail_refund_payment' ? 'sale-1'
      : item.kind === 'pos_child_tail_completed_line' ? facts.id : facts) as never,
  );

describe('native POS child-tail read bridge (round 30)', () => {
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
    expect(await invoke(item, mockDb(seen))).toEqual(rows[0]);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44997/api/v1/companies//pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: '', kind: item.kind, values: [...item.values] });
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
    await expect(invoke(item, {})).rejects.toMatchObject({ code: 'NATIVE_POS_OWNER_UNAVAILABLE', status: 503 });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps no retired child-tail SQL literal in the TS adapters', () => {
    const core = readFileSync(new URL('../../../modules/pos/server/src/adapters/pos-core.ts', import.meta.url), 'utf8');
    for (const item of cases) expect(core).not.toContain(item.sql);
    expect(core.match(/nativePosChildTail(LinesSum|CreatedBy|CompletedLine|RefundPayment)Statement\(db/g)).toHaveLength(4);
  });

  it('keeps row locks, PMS fragments and day-report/Stripe registries inline', () => {
    const core = readFileSync(new URL('../../../modules/pos/server/src/adapters/pos-core.ts', import.meta.url), 'utf8');
    const pos = readFileSync(new URL('../../../modules/pos/server/src/adapters/pos.ts', import.meta.url), 'utf8');
    const dayReport = readFileSync(new URL('../../../modules/pos/server/src/adapters/native-pos-day-report.ts', import.meta.url), 'utf8');
    const stripe = readFileSync(new URL('../../../modules/pos/server/src/adapters/native-pos-stripe-reads.ts', import.meta.url), 'utf8');
    expect(core).toContain("const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';");
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(pos).toContain('${pmsProjectionSources.pms_sources}');
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(dayReport).toContain("'pos_day_report_");
    expect(stripe).toContain("'pos_stripe_");
  });
});
