import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativePosSaleLinesStatement,
  nativePosSalePaymentsStatement,
  nativePosLinesBySaleStatement,
  nativePosSucceededPaymentTotalStatement,
  nativePosPendingPaymentIdStatement,
  nativePosRefundablePaymentsStatement,
  nativePosPaymentIdBySaleStatement,
  nativePosFulfilledLinesStatement,
  nativePosFirstPaymentIdStatement,
  nativePosLinesTotalStatement,
  nativePosOpenPaymentIdStatement,
  nativePosDayPlannerLinesStatement,
  nativePosDayPlannerPaymentsStatement,
} from '../../../packages/server-pos/src/adapters/native-pos-sale-child';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
const rows = [{ id: 'row' }];
const saleId = 'sale-1';

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
    name: 'workspace sale lines with fulfillment',
    kind: 'pos_sale_child_lines_with_fulfillment',
    sql: 'SELECT l.*,f.state fulfillment_state,f.version fulfillment_version,f.kind,f.cost_minor fulfillment_cost FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id',
    bridge: nativePosSaleLinesStatement,
  },
  {
    name: 'sale payments in created order',
    kind: 'pos_sale_child_payments_ordered',
    sql: 'SELECT * FROM pos_payments WHERE sale_id=? ORDER BY created_at',
    bridge: nativePosSalePaymentsStatement,
  },
  {
    name: 'all plain sale lines',
    kind: 'pos_sale_child_lines_all',
    sql: 'SELECT * FROM pos_lines WHERE sale_id=?',
    bridge: nativePosLinesBySaleStatement,
  },
  {
    name: 'succeeded payment net total',
    kind: 'pos_sale_child_payments_succeeded_total',
    sql: "SELECT COALESCE(SUM(amount_minor-refunded_minor),0) amount FROM pos_payments WHERE sale_id=? AND state='succeeded'",
    bridge: nativePosSucceededPaymentTotalStatement,
    first: true,
  },
  {
    name: 'pending or exception payment id',
    kind: 'pos_sale_child_payment_pending_id',
    sql: "SELECT id FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','exception')",
    bridge: nativePosPendingPaymentIdStatement,
    first: true,
  },
  {
    name: 'refundable succeeded payments',
    kind: 'pos_sale_child_payments_refundable',
    sql: "SELECT * FROM pos_payments WHERE sale_id=? AND state='succeeded' AND amount_minor>refunded_minor",
    bridge: nativePosRefundablePaymentsStatement,
  },
  {
    name: 'plain payment id',
    kind: 'pos_sale_child_payment_id',
    sql: 'SELECT id FROM pos_payments WHERE sale_id=?',
    bridge: nativePosPaymentIdBySaleStatement,
    first: true,
  },
  {
    name: 'fulfilled sale line detail',
    kind: 'pos_sale_child_lines_fulfillment_detail',
    sql: 'SELECT l.*,f.kind,f.state fulfillment_state,f.cost_minor fulfillment_cost,f.quantity_milli fulfillment_quantity,f.warehouse_id,f.product_id FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id',
    bridge: nativePosFulfilledLinesStatement,
  },
  {
    name: 'first payment id',
    kind: 'pos_sale_child_payment_first_id',
    sql: 'SELECT id FROM pos_payments WHERE sale_id=? LIMIT 1',
    bridge: nativePosFirstPaymentIdStatement,
    first: true,
  },
  {
    name: 'sale lines gross total',
    kind: 'pos_sale_child_lines_total',
    sql: 'SELECT COALESCE(SUM(subtotal_minor+tax_minor+discount_minor),0) n FROM pos_lines WHERE sale_id=?',
    bridge: nativePosLinesTotalStatement,
    first: true,
  },
  {
    name: 'open or refundable payment id',
    kind: 'pos_sale_child_payment_open_id',
    sql: "SELECT id FROM pos_payments WHERE sale_id=? AND (state IN ('requested','pending','exception') OR state='succeeded' AND amount_minor>refunded_minor)",
    bridge: nativePosOpenPaymentIdStatement,
    first: true,
  },
  {
    name: 'day-planner receipt sale lines',
    kind: 'pos_sale_child_day_planner_lines',
    sql: 'SELECT id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,note FROM pos_lines WHERE sale_id=? ORDER BY id',
    bridge: nativePosDayPlannerLinesStatement,
  },
  {
    name: 'day-planner succeeded sale payments',
    kind: 'pos_sale_child_day_planner_payments',
    sql: "SELECT id,method,amount_minor,tendered_minor,reference,finance_payment_id FROM pos_payments WHERE sale_id=? AND state='succeeded' ORDER BY created_at,id",
    bridge: nativePosDayPlannerPaymentsStatement,
  },
] as const;

describe('native POS sale-child detail read bridge', () => {
  it.each(cases)('requests the $name from the native owner registry only', async (item) => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql: item.sql, values: [saleId] } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const result = await (item.bridge as (db: never, saleId: unknown) => Promise<unknown>)(
      mockDb(seen) as never, saleId,
    );
    expect(result).toEqual('first' in item && item.first ? rows[0] : rows);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies//pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: '', kind: item.kind, values: [saleId] });
    expect(seen.sql).toBe(item.sql);
    expect(seen.values).toEqual([saleId]);
    expect(seen.sql).toMatch(/^SELECT /);
    expect(seen.sql).toContain('sale_id=?');
    expect(seen.sql).not.toMatch(/FOR UPDATE|FOR SHARE|FOR NO KEY UPDATE|LOCK TABLE|pg_advisory|INSERT|UPDATE |DELETE|RETURNING|pms_sources|pms_properties/);
    expect(seen.sql!.match(/\?/g)?.length).toBe(1);
  });

  it.each(cases)('fails closed for $name when the native owner service is unconfigured', async (item) => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(
      (item.bridge as (db: never, saleId: unknown) => Promise<unknown>)({} as never, saleId),
    ).rejects.toMatchObject({ code: 'NATIVE_POS_OWNER_UNAVAILABLE', status: 503 });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps no retired sale-child SQL literal in the TS adapters', () => {
    const pos = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos.ts', import.meta.url), 'utf8');
    const core = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-core.ts', import.meta.url), 'utf8');
    const daily = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-daily.ts', import.meta.url), 'utf8');
    for (const item of cases) {
      expect(pos).not.toContain(item.sql);
      expect(core).not.toContain(item.sql);
      expect(daily).not.toContain(item.sql);
    }
    expect(pos.match(/nativePosSale(Lines|Payments)Statement\(db/g)).toHaveLength(2);
    expect(core.match(/nativePos(LinesBySale|SucceededPaymentTotal|PendingPaymentId|RefundablePayments|PaymentIdBySale|FulfilledLines|FirstPaymentId|LinesTotal|OpenPaymentId)Statement\(db/g)).toHaveLength(9);
    expect(daily.match(/nativePosDayPlanner(Lines|Payments)Statement\(db/g)).toHaveLength(2);
  });

  it('mounts both day-planner kinds in the Rust registry with byte parity and pins r36 KPIs', () => {
    const registry = readFileSync(new URL('../../../packages/server-pos/src/adapters/sale_child_reads.rs', import.meta.url), 'utf8');
    const owner = readFileSync(new URL('../../../packages/server-pos/src/adapters/owner_writes.rs', import.meta.url), 'utf8');
    expect(registry).toContain('assert_eq!(KINDS.len(), 13)');
    expect(registry).toContain('assert_eq!(EXPECTED.len(), 13)');
    expect(owner).toContain('assert_eq!(KINDS.len(), 196)');
    expect(owner).toContain('assert_eq!(crate::adapters::sale_child_reads::KINDS.len(), 13)');
    for (const item of cases) {
      expect(registry).toContain(`"${item.sql}"`);
      expect(registry).toContain(`"${item.kind}"`);
      expect(item.sql.match(/\?/g)?.length).toBe(1);
    }
    for (const name of ['DAY_PLANNER_LINES', 'DAY_PLANNER_PAYMENTS'])
      expect(owner).toContain(`crate::adapters::sale_child_reads::${name},`);
  });

  it('keeps lock and PMS-fragment reads inline; refund-payment tail, document and reversal reads are native', () => {
    const pos = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos.ts', import.meta.url), 'utf8');
    const core = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-core.ts', import.meta.url), 'utf8');
    const daily = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-daily.ts', import.meta.url), 'utf8');
    expect(daily).toContain('${pmsProjectionSources.pms_sources}');
    expect(daily).toContain('SELECT * FROM pos_day_closes WHERE id=? FOR UPDATE');
    expect(pos).toContain('${pmsProjectionSources.pms_sources}');
    expect(core).toContain('SELECT * FROM pos_sales WHERE id=? FOR UPDATE');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).not.toContain("SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE p.sale_id=? AND rp.state<>'succeeded'");
    expect(core).toContain('nativePosChildTailRefundPaymentStatement');
    expect(pos).not.toContain('SELECT snapshot_json::text FROM pos_sale_documents WHERE sale_id=? AND company_id=?');
    expect(pos).toContain('nativePosKeyedSaleDocumentStatement');
    expect(core).not.toContain('SELECT sale_id FROM pos_room_charge_reversals WHERE sale_id=?');
    expect(core).toContain('nativePosKeyedReversalBySaleStatement');
    expect(core).toContain('SELECT * FROM pos_room_charge_reversal_requests WHERE id=? FOR UPDATE');
  });
});
