import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import {
  nativeInventoryReadAll,
  INVENTORY_RD_LANDED_COST_DETAIL_LINES,
  INVENTORY_RD_LANDED_COST_DETAIL_LOTS,
  INVENTORY_RD_LANDED_COST_DETAIL_EVENTS,
} from '../../../packages/server-inventory/src/adapters/native-inventory-read';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

/** Byte oracles captured verbatim from the HEAD TypeScript literals (r49 base). */
const RETIRED = {
  INVENTORY_RD_LANDED_COST_DETAIL_LINES: 'SELECT l.*,p.sku,p.name,p.unit,p.tracking_mode FROM inventory_landed_cost_lines l JOIN commerce_products p ON p.id=l.product_id WHERE l.landed_cost_id=? ORDER BY l.ordinal',
  INVENTORY_RD_LANDED_COST_DETAIL_LOTS: 'SELECT x.*,l.lot_code,l.serial_number FROM inventory_landed_cost_lots x JOIN inventory_lots l ON l.id=x.lot_id JOIN inventory_landed_cost_lines line ON line.id=x.landed_cost_line_id WHERE line.landed_cost_id=? ORDER BY line.ordinal,x.id',
  INVENTORY_RD_LANDED_COST_DETAIL_EVENTS: 'SELECT * FROM inventory_landed_cost_events WHERE landed_cost_id=? ORDER BY created_at,id',
} as const;

describe('native Inventory landed-cost detail child reads', () => {
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
    const rows = [{ id: 'child1', landed_cost_id: 'cost1' }];
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
    [INVENTORY_RD_LANDED_COST_DETAIL_LINES, ['cost1']],
    [INVENTORY_RD_LANDED_COST_DETAIL_LOTS, ['cost1']],
    [INVENTORY_RD_LANDED_COST_DETAIL_EVENTS, ['cost1']],
  ] as const;

  it('routes each landed-cost detail child read through its native kind with caller-unit binds and .all shape', async () => {
    for (const [kind, values] of cases) {
      const bodies: { kind: string; values: unknown[] }[] = [];
      planFetch(bodies);
      const { db, bound, rows } = fakeDb();
      const result = await nativeInventoryReadAll(kind, values, db as never);
      expect((result as { results: unknown[] }).results).toEqual(rows);
      expect(bodies.map((x) => x.kind)).toEqual([kind]);
      expect(bodies.map((x) => x.values)).toEqual([[...values]]);
      expect(bound).toEqual([[...values]]);
    }
  });

  it('fails closed for a landed-cost detail child read when the native read service is unavailable', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('down', { status: 503 })));
    const { db } = fakeDb();
    await expect(nativeInventoryReadAll(INVENTORY_RD_LANDED_COST_DETAIL_LOTS, ['cost1'], db as never)).rejects.toMatchObject({ status: 503 });
  });

  it('TS landed-cost adapter carries no retired SQL and keeps the Promise.all / .results shape', () => {
    const detail = src('adapters/inventory-landed-costs.ts');
    for (const lit of Object.values(RETIRED)) expect(detail).not.toContain(lit);
    expect(detail).toContain('INVENTORY_RD_LANDED_COST_DETAIL_LINES');
    expect(detail).toContain('INVENTORY_RD_LANDED_COST_DETAIL_LOTS');
    expect(detail).toContain('INVENTORY_RD_LANDED_COST_DETAIL_EVENTS');
    expect(detail).toContain('Promise.all([nativeInventoryReadAll<LandedCostRow>(INVENTORY_RD_LANDED_COST_DETAIL_LINES');
    expect(detail).toContain('lines:lines.results,lots:lots.results,events:events.results');
    expect(detail).toContain('requireLandedCostPurchasedRead(write,actor,id)');
    // Bridge emits no SQL of its own.
    const bridge = src('adapters/native-inventory-read.ts');
    for (const lit of Object.values(RETIRED)) expect(bridge).not.toContain(lit);
  });

  it('registered native SQL is byte-identical to the retired TypeScript literals with holding scope and one bind', () => {
    const rust = src('adapters/native_command_sql.rs');
    for (const [name, sql] of Object.entries(RETIRED)) {
      const suffix = name.replace('INVENTORY_RD_', '');
      const constName = name.replace('INVENTORY_RD_', 'SQL_RD_');
      const re = new RegExp('const ' + constName + ': &str =\\s*(?:\\n\\s*)?"((?:[^"\\\\]|\\\\.)*)"');
      const m = rust.match(re);
      expect(m, constName).not.toBeNull();
      expect(m![1].replace(/\\"/g, '"'), constName).toBe(sql);
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
    walk(srcRoot);
    // r49 ended at 307 (2 commerce-facts + 5 inventory-transit reads retired).
    // r50 retires 2 landed-cost detail child reads (lot join, events): 307 -> 305.
    // r51 retires 2 transit detail-lot reads (transitLots join, selectableTransitLots): 305 -> 303.
    // r52 retires 1 landed-cost detail-lines read: 303 -> 302.
    // r53 folds 3 byte-identical count-list projection heads into one helper: 302 -> 300.
    // r54 folds 3 package-obligation stock heads into one kind-parameterized renderer: 300 -> 298.
    expect(total).toBe(277);
    expect(total).toBeLessThan(299);
  });
});
