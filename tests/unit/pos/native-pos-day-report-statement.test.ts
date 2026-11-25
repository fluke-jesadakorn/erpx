import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativePosDayClosesStatement,
  nativePosDaySalesStatement,
  nativePosDayShiftsStatement,
  nativePosDayPaymentsStatement,
  nativePosDayRefundsStatement,
  nativePosDayRefundPaymentsStatement,
  nativePosDayStockMovesStatement,
  nativePosDayFulfillmentsStatement,
  nativePosDayCashMovementsStatement,
  nativePosDaySettlementsStatement,
  nativePosPaymentMixStatement,
} from '../../../packages/server-pos/src/adapters/native-pos-day-report';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
const scope = { company: 'company', branch: 'branch', date: '2026-01-01' };
const rows = [{ id: 'row' }];

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

const PMS_SOURCES = 'SELECT "property_id","source_key","record_id" FROM pms_sources';
const COMMERCE_WAREHOUSES = 'SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses';

const cases = [
  {
    name: 'day-close existence',
    kind: 'pos_day_report_day_closes',
    sql: 'SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=?',
    values: ['company', 'branch', '2026-01-01'],
    bridge: nativePosDayClosesStatement,
    args: [scope] as const,
    first: true,
  },
  {
    name: 'business-date sales',
    kind: 'pos_day_report_sales',
    sql: `SELECT s.*,ps.record_id room_charge_id,rr.reversal_entry_id FROM pos_sales s LEFT JOIN (${PMS_SOURCES}) ps ON ps.source_key='pos:'||s.id||':'||s.id LEFT JOIN pos_room_charge_reversals rr ON rr.sale_id=s.id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY s.id`,
    values: ['company', 'branch', '2026-01-01'],
    bridge: nativePosDaySalesStatement,
    args: [scope] as const,
  },
  {
    name: 'business-date shifts',
    kind: 'pos_day_report_shifts',
    sql: 'SELECT * FROM pos_shifts WHERE company_id=? AND branch_id=? AND business_date=? ORDER BY id',
    values: ['company', 'branch', '2026-01-01'],
    bridge: nativePosDayShiftsStatement,
    args: [scope] as const,
  },
  {
    name: 'business-date payments',
    kind: 'pos_day_report_payments',
    sql: 'SELECT p.*,s.number sale_number FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY p.id',
    values: ['company', 'branch', '2026-01-01'],
    bridge: nativePosDayPaymentsStatement,
    args: [scope] as const,
  },
  {
    name: 'accounting-date refunds',
    kind: 'pos_day_report_refunds',
    sql: 'SELECT r.*,s.number sale_number FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY r.id',
    values: ['company', 'branch', '2026-01-01'],
    bridge: nativePosDayRefundsStatement,
    args: [scope] as const,
  },
  {
    name: 'accounting-date refund payments',
    kind: 'pos_day_report_refund_payments',
    sql: 'SELECT rp.*,p.method FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY rp.id',
    values: ['company', 'branch', '2026-01-01'],
    bridge: nativePosDayRefundPaymentsStatement,
    args: [scope] as const,
  },
  {
    name: 'movement-date stock moves',
    kind: 'pos_day_report_stock_moves',
    sql: `SELECT * FROM pos_stock_moves WHERE company_id=? AND movement_date=? AND warehouse_id IN (SELECT id FROM (${COMMERCE_WAREHOUSES}) commerce_warehouses WHERE company_id=? AND branch_id=?) ORDER BY id`,
    values: ['company', '2026-01-01', 'company', 'branch'],
    bridge: nativePosDayStockMovesStatement,
    args: ['company', '2026-01-01', 'company', 'branch'] as const,
  },
  {
    name: 'business-date fulfillments',
    kind: 'pos_day_report_fulfillments',
    sql: "SELECT id,state FROM pos_fulfillments WHERE company_id=? AND branch_id=? AND business_date=? AND state IN ('queued','preparing','ready') ORDER BY id",
    values: ['company', 'branch', '2026-01-01'],
    bridge: nativePosDayFulfillmentsStatement,
    args: [scope] as const,
  },
  {
    name: 'business-date cash movements',
    kind: 'pos_day_report_cash_movements',
    sql: 'SELECT m.id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND m.approved_by IS NULL ORDER BY m.id',
    values: ['company', 'branch', '2026-01-01'],
    bridge: nativePosDayCashMovementsStatement,
    args: [scope] as const,
  },
  {
    name: 'company settlements',
    kind: 'pos_day_report_settlements',
    sql: "SELECT id,kind,state,amount_minor,fee_minor,amount_minor-fee_minor net_minor FROM pos_settlements WHERE company_id=? AND ((kind='payout' AND state<>'matched') OR (kind='balance' AND state='unmapped')) ORDER BY id",
    values: ['company'],
    bridge: nativePosDaySettlementsStatement,
    args: ['company'] as const,
  },
  {
    name: 'business-date payment mix with refunded sums',
    kind: 'pos_day_report_payment_mix',
    sql: `SELECT s.business_date,s.register_id,s.shift_id,s.currency,p.method,SUM(p.amount_minor) received_minor,
 SUM(COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp WHERE rp.payment_id=p.id AND rp.state='succeeded'),0)) refunded_minor
 FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND p.state='succeeded'
 GROUP BY s.business_date,s.register_id,s.shift_id,s.currency,p.method ORDER BY s.business_date DESC LIMIT 300`,
    values: ['company', 'branch', 'branch'],
    bridge: nativePosPaymentMixStatement,
    args: [{ company: 'company', branch: 'branch', branch2: 'branch' }] as const,
  },
] as const;

describe('native POS day-report business-date list read bridge', () => {
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
    const result = await (item.bridge as (db: never, ...rest: unknown[]) => Promise<unknown>)(
      mockDb(seen) as never, ...item.args,
    );
    expect(result).toEqual('first' in item && item.first ? rows[0] : rows);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('company');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind: item.kind, values: item.values });
    expect(seen.sql).toBe(item.sql);
    expect(seen.values).toEqual(item.values);
    expect(seen.sql).toMatch(/^SELECT /);
    expect(seen.sql).not.toMatch(/FOR UPDATE|FOR SHARE|FOR NO KEY UPDATE|LOCK TABLE|pg_advisory|INSERT|UPDATE |DELETE|RETURNING/);
    expect(seen.sql!.match(/\?/g)?.length).toBe(item.values.length);
  });

  it.each(cases)('fails closed for $name when the native owner service is unconfigured', async (item) => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(
      (item.bridge as (db: never, ...rest: unknown[]) => Promise<unknown>)({} as never, ...item.args),
    ).rejects.toMatchObject({ code: 'NATIVE_POS_OWNER_UNAVAILABLE', status: 503 });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps no byte-identical day-report SQL literal in the TS adapter', () => {
    const source = readFileSync(
      new URL('../../../packages/server-pos/src/adapters/pos-daily.ts', import.meta.url),
      'utf8',
    );
    for (const item of cases) {
      if (item.kind === 'pos_day_report_sales' || item.kind === 'pos_day_report_stock_moves') continue;
      expect(source).not.toContain(item.sql);
    }
    expect(source).not.toContain('SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=?');
    expect(source).not.toContain('SELECT * FROM pos_shifts WHERE company_id=? AND branch_id=? AND business_date=?');
    expect(source).not.toContain('SELECT id,state FROM pos_fulfillments WHERE company_id=?');
    expect(source.match(/nativePosDay(?!Planner)[A-Za-z]+Statement\(db/g)).toHaveLength(10);
    expect(source.match(/nativePosDayPlanner(Lines|Payments)Statement\(db/g)).toHaveLength(2);
    const pos = readFileSync(
      new URL('../../../packages/server-pos/src/adapters/pos.ts', import.meta.url),
      'utf8',
    );
    expect(pos).not.toContain('SUM(p.amount_minor) received_minor');
    expect(pos).toContain('nativePosPaymentMixStatement(db, facts)');
  });

  it('keeps lock-bearing and PMS point-lookup reads inline in pos-daily.ts', () => {
    const source = readFileSync(
      new URL('../../../packages/server-pos/src/adapters/pos-daily.ts', import.meta.url),
      'utf8',
    );
    expect(source).toContain('SELECT * FROM pos_day_closes WHERE id=? FOR UPDATE');
    expect(source).toContain('${pmsProjectionSources.pms_sources}');
  });
});
