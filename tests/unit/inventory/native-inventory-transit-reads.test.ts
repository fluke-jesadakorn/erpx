import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import {
  nativeInventoryReadAll,
  nativeInventoryReadFirst,
  INVENTORY_RD_ORDER_RESERVATIONS_BY_LINE,
  INVENTORY_RD_LOT_BALANCE_BY_LOT_WAREHOUSE,
  INVENTORY_RD_TRANSIT_EVENTS_WITH_RESOLUTION,
  INVENTORY_RD_POSTED_TRANSIT_TRANSFER,
  INVENTORY_RD_TRANSFER_BY_ID,
  INVENTORY_RD_TRANSIT_DISPATCH_LINES,
  INVENTORY_RD_TRANSIT_DETAIL_LOTS,
  INVENTORY_RD_SELECTABLE_TRANSIT_LOTS,
} from '../../../packages/server-inventory/src/adapters/native-inventory-read';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src/adapters');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const srcDir = resolve(__dirname, '../../../packages/server-inventory/src');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

/** Byte oracles captured verbatim from the retired HEAD TypeScript literals. */
const RETIRED = {
  INVENTORY_RD_ORDER_RESERVATIONS_BY_LINE: 'SELECT * FROM inventory_order_reservations WHERE order_line_id=? AND quantity_milli>consumed_milli+released_milli AND expires_at>floor(extract(epoch FROM statement_timestamp())*1000)::bigint ORDER BY created_at,id',
  INVENTORY_RD_LOT_BALANCE_BY_LOT_WAREHOUSE: 'SELECT * FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?',
  INVENTORY_RD_TRANSIT_EVENTS_WITH_RESOLUTION: 'SELECT e.*,(SELECT r.kind FROM inventory_transit_events r WHERE r.proposal_id=e.id) resolution FROM inventory_transit_events e WHERE e.transit_id=? ORDER BY e.created_at,e.id',
  INVENTORY_RD_POSTED_TRANSIT_TRANSFER: "SELECT id,version FROM inventory_transfers WHERE id=? AND company_id=? AND status='posted' AND delivery_mode='transit'",
  INVENTORY_RD_TRANSFER_BY_ID: 'SELECT * FROM inventory_transfers WHERE id=?',
  INVENTORY_RD_TRANSIT_DISPATCH_LINES: 'SELECT l.*,s.source_version,s.source_quantity_milli,s.source_value_minor,s.product_version FROM inventory_transit_lines l JOIN inventory_transfer_lines s ON s.id=l.source_line_id WHERE l.transit_id=? ORDER BY l.id',
  INVENTORY_RD_TRANSIT_DETAIL_LOTS: "SELECT tl.*,l.lot_code,l.serial_number,l.expiry_date,COALESCE((SELECT SUM(le.quantity_milli) FROM inventory_transit_lot_events le JOIN inventory_transit_events e ON e.id=le.event_id WHERE le.transit_lot_id=tl.id AND e.kind='loss_requested' AND NOT EXISTS(SELECT 1 FROM inventory_transit_events r WHERE r.proposal_id=e.id)),0) pending_loss_milli FROM inventory_transit_lots tl JOIN inventory_lots l ON l.id=tl.lot_id JOIN inventory_transit_lines line ON line.id=tl.transit_line_id WHERE line.transit_id=? ORDER BY line.id,l.expiry_date NULLS LAST,l.id",
  INVENTORY_RD_SELECTABLE_TRANSIT_LOTS: "SELECT tl.*,COALESCE((SELECT SUM(le.quantity_milli) FROM inventory_transit_lot_events le JOIN inventory_transit_events e ON e.id=le.event_id WHERE le.transit_lot_id=tl.id AND e.kind='loss_requested' AND NOT EXISTS(SELECT 1 FROM inventory_transit_events r WHERE r.proposal_id=e.id)),0) pending_loss_milli FROM inventory_transit_lots tl WHERE tl.transit_line_id=? ORDER BY tl.id",
} as const;

describe('native Inventory inventory-transit / commerce-facts fixed reads', () => {
  const envKey = 'ERPX_NATIVE_INVENTORY_CONFIG';
  const original = process.env[envKey];
  beforeEach(() => {
    process.env[envKey] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44991',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
    });
  });
  afterEach(() => {
    if (original === undefined) delete process.env[envKey];
    else process.env[envKey] = original;
    vi.unstubAllGlobals();
  });

  const fakeDb = () => {
    const bound: unknown[][] = [];
    const rows = [{ id: 'r1', version: 1 }];
    const db = {
      prepare(_sql: string) {
        return { bind(...values: unknown[]) { bound.push(values); return this; }, async all() { return { results: rows, success: true, meta: { changes: 0, duration: 0 } }; }, async first() { return rows[0] ?? null; } };
      },
      async batch() { return [{ results: rows, success: true, meta: { changes: 0, duration: 0 } }]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    return { db, bound, rows };
  };

  const planFetch = (bodies: { kind: string; values: unknown[] }[]) =>
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      return new Response(JSON.stringify({ data: { statements: [{ sql: 'SELECT 1', values: body.values }] } }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));

  const cases = [
    [INVENTORY_RD_ORDER_RESERVATIONS_BY_LINE, ['line1'], false],
    [INVENTORY_RD_LOT_BALANCE_BY_LOT_WAREHOUSE, ['lot1', 'w1'], true],
    [INVENTORY_RD_TRANSIT_EVENTS_WITH_RESOLUTION, ['transit1'], false],
    [INVENTORY_RD_POSTED_TRANSIT_TRANSFER, ['transfer1', 'company'], true],
    [INVENTORY_RD_TRANSFER_BY_ID, ['transfer1'], true],
    [INVENTORY_RD_TRANSIT_DISPATCH_LINES, ['transit1'], false],
    [INVENTORY_RD_TRANSIT_DETAIL_LOTS, ['transit1'], false],
    [INVENTORY_RD_SELECTABLE_TRANSIT_LOTS, ['line1'], false],
  ] as const;

  it('routes each retired fixed read through its native kind with caller-unit binds and shape', async () => {
    for (const [kind, values, first] of cases) {
      const bodies: { kind: string; values: unknown[] }[] = [];
      planFetch(bodies);
      const { db, bound, rows } = fakeDb();
      const result = first
        ? await nativeInventoryReadFirst(kind, values, db as never)
        : await nativeInventoryReadAll(kind, values, db as never);
      if (first) expect(result).toEqual(rows[0]);
      else expect((result as { results: unknown[] }).results).toEqual(rows);
      expect(bodies.map((x) => x.kind)).toEqual([kind]);
      expect(bodies.map((x) => x.values)).toEqual([[...values]]);
      expect(bound).toEqual([[...values]]);
    }
  });

  it('fails closed for a fixed read when the native read service is unavailable', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('down', { status: 503 })));
    const { db } = fakeDb();
    await expect(nativeInventoryReadAll(INVENTORY_RD_TRANSIT_EVENTS_WITH_RESOLUTION, ['transit1'], db as never)).rejects.toMatchObject({ status: 503 });
  });

  it('TS adapters carry no retired SQL literals and reference the native kinds', () => {
    const facts = src('commerce-facts.ts');
    const transit = src('inventory-transit.ts');
    for (const [, lit] of Object.entries(RETIRED)) {
      expect(facts + transit).not.toContain(lit);
    }
    expect(facts).toContain('INVENTORY_RD_ORDER_RESERVATIONS_BY_LINE');
    expect(facts).toContain('INVENTORY_RD_LOT_BALANCE_BY_LOT_WAREHOUSE');
    expect(transit).toContain('INVENTORY_RD_TRANSIT_EVENTS_WITH_RESOLUTION');
    expect(transit).toContain('INVENTORY_RD_POSTED_TRANSIT_TRANSFER');
    expect(transit).toContain('INVENTORY_RD_TRANSFER_BY_ID');
    expect(transit).toContain('INVENTORY_RD_TRANSIT_DISPATCH_LINES');
    expect(transit).toContain('INVENTORY_RD_TRANSIT_DETAIL_LOTS');
    expect(transit).toContain('INVENTORY_RD_SELECTABLE_TRANSIT_LOTS');
    // Bridge emits no SQL of its own.
    const bridge = src('native-inventory-read.ts');
    for (const lit of Object.values(RETIRED)) expect(bridge).not.toContain(lit);
  });

  it('registered native SQL is byte-identical to the retired TypeScript literals', () => {
    const rust = src('native_command_sql.rs');
    for (const [name, sql] of Object.entries(RETIRED)) {
      const constName = name.replace('INVENTORY_RD_', 'SQL_RD_').toUpperCase();
      const re = new RegExp('const ' + constName + ': &str =\\s*(?:\\n\\s*)?"((?:[^"\\\\]|\\\\.)*)"');
      const m = rust.match(re);
      expect(m, constName).not.toBeNull();
      expect(m![1].replace(/\\"/g, '"'), constName).toBe(sql);
    }
    for (const name of ['INVENTORY_RD_TRANSIT_DETAIL_LOTS', 'INVENTORY_RD_SELECTABLE_TRANSIT_LOTS']) {
      const suffix = name.replace('INVENTORY_RD_', '');
      const block = rust.match(new RegExp('const READ_' + suffix + ': NativeStatement = NativeStatement \\{([\\s\\S]*?)\\};'));
      expect(block, suffix).not.toBeNull();
      expect(block![1]).toContain(`kind: RD_${suffix},`);
      expect(block![1]).toContain('bind_count: 1');
      expect(block![1]).toContain('scope: "holding"');
      expect(rust).toContain(`| RD_${suffix}`);
      expect(rust).toContain(`RD_${suffix} => Some(&READ_${suffix})`);
    }
  });

  it('retirement:ts-statements KPI is 300 after the r53 count-list head fold (strictly below r52 302)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcDir);
    // r49 ended at 307; r50 retired 2 landed-cost detail child reads: 307 -> 305.
    // r51 retires 2 transit detail-lot reads (transitLots join, selectableTransitLots): 305 -> 303.
    // r52 retires 1 landed-cost detail-lines read: 303 -> 302.
    // r53 folds 3 byte-identical count-list projection heads into one helper: 302 -> 300.
    // r54 folds 3 package-obligation stock heads into one kind-parameterized renderer: 300 -> 298.
    expect(total).toBe(294);
    expect(total).toBeLessThan(300);
  });
});
