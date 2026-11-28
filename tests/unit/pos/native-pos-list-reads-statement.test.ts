import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativePosSalesListStatement,
  nativePosRefundsListStatement,
  nativePosConnectionsRowStatement,
  nativePosSettlementsListStatement,
  nativePosExceptionJobsListStatement,
  nativePosRecordSupplementSalesStatement,
} from '../../../modules/pos/server/src/adapters/native-pos-list-reads';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44997', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
const rows = [{ id: 'row' }];
const scope = { company: 'co', branch: 'br', branch2: null };

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

const cases = [
  {
    name: 'workspace sales list',
    kind: 'pos_list_sales',
    sql: "SELECT s.*,r.name register_name FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?='' OR s.number ILIKE ?) AND (?=0 OR s.state IN ('open','held','payment_pending')) ORDER BY s.created_at DESC LIMIT 150",
    values: ['co', 'br', null, 'hist', '%hist%', 1],
    bridge: nativePosSalesListStatement,
    arg: { ...scope, fact3: 'hist', fact4: '%hist%', fact5: 1 as const },
    list: true,
  },
  {
    name: 'workspace refunds list',
    kind: 'pos_list_refunds',
    sql: 'SELECT f.*,s.number sale_number,s.currency FROM pos_refunds f JOIN pos_sales s ON s.id=f.sale_id WHERE f.company_id=? AND (?::text IS NULL OR f.branch_id=?) AND (?::text IS NULL OR f.sale_id=?) ORDER BY f.created_at DESC LIMIT 100',
    values: ['co', 'br', null, 'r1', 'r1'],
    bridge: nativePosRefundsListStatement,
    arg: { ...scope, fact3: 'r1', fact4: 'r1' },
    list: true,
  },
  {
    name: 'PMS booking connection row',
    kind: 'pos_list_connections',
    sql: "SELECT * FROM pos_connections WHERE company_id=? AND state='ready'",
    values: ['co'],
    bridge: nativePosConnectionsRowStatement,
    arg: 'co',
    list: false,
  },
  {
    name: 'settlements list',
    kind: 'pos_list_settlements',
    sql: 'SELECT * FROM pos_settlements WHERE company_id=? ORDER BY created_at DESC LIMIT 100',
    values: ['co'],
    bridge: nativePosSettlementsListStatement,
    arg: 'co',
    list: true,
  },
  {
    name: 'exception/refund-connect jobs list',
    kind: 'pos_list_exception_jobs',
    sql: "SELECT id,kind,record_id,state,last_error,payload_json,created_at,(SELECT r.version FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id WHERE rp.id=pos_jobs.record_id AND r.company_id=pos_jobs.company_id AND pos_jobs.kind='refund') refund_version FROM pos_jobs WHERE company_id=? AND (state='exception' OR kind IN ('connect','connect_refresh')) ORDER BY created_at DESC LIMIT 50",
    values: ['co'],
    bridge: nativePosExceptionJobsListStatement,
    arg: 'co',
    list: true,
  },
] as const;

describe('native POS list-read bridge (round 31)', () => {
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
    const result = await (item.bridge as (db: never, arg: never) => Promise<unknown>)(
      mockDb(seen) as never,
      item.arg as never,
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
      (item.bridge as (db: never, arg: never) => Promise<unknown>)({} as never, item.arg as never),
    ).rejects.toMatchObject({ code: 'NATIVE_POS_OWNER_UNAVAILABLE', status: 503 });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('requests the POS-owned record-supplement sale ids from the owner registry', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const sql = 'SELECT DISTINCT s.id FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (s.id=? OR EXISTS(SELECT 1 FROM pos_lines l WHERE l.sale_id=s.id AND l.fulfillment_id=?) OR EXISTS(SELECT 1 FROM pos_refunds r WHERE r.sale_id=s.id AND r.id=?))';
    const values = ['co', 'br', 'br', 'sale', 'line', 'refund'];
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql, values } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const result = await nativePosRecordSupplementSalesStatement(mockDb(seen) as never, {
      companyId: 'co', branchId: 'br', branchId2: 'br',
      sourceId: 'sale', sourceId2: 'line', sourceId3: 'refund',
    });
    expect(result.results).toEqual(rows);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0] as [string, RequestInit & { body: string }];
    expect(url).toBe('http://127.0.0.1:44997/api/v1/companies/co/pos/owner-write');
    expect(JSON.parse(init.body)).toEqual({ company: 'co', kind: 'pos_list_record_supplement_sales', values });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(values);
    expect(seen.sql!.match(/\?/g)).toHaveLength(values.length);
    expect(seen.sql).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE|pg_advisory|pms_sources|commerce_/);
  });

  it('keeps no retired list-read SQL literal in the TS adapters', () => {
    const pos = readFileSync(new URL('../../../modules/pos/server/src/adapters/pos.ts', import.meta.url), 'utf8');
    for (const item of cases) expect(pos).not.toContain(item.sql);
    expect(pos.match(/nativePos(SalesList|RefundsList|ConnectionsRow|SettlementsList|ExceptionJobsList)Statement\(db/g)).toHaveLength(5);
    const records = readFileSync(new URL('../../../modules/pos/server/src/adapters/records-record-details-owned-facts.ts', import.meta.url), 'utf8');
    expect(records).not.toContain('SELECT DISTINCT s.id FROM pos_sales');
    expect(records).toContain('nativePosRecordSupplementSalesStatement<Row>(database');
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
