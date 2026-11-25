import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativeManufacturingPlan,
  nativeManufacturingReadAll,
  MANUFACTURING_RD_ORDERS,
  MANUFACTURING_RD_SPECIFICATION,
  MANUFACTURING_RD_ORDER_EVENTS,
  MANUFACTURING_RD_ORDER_SPECIFICATION_LINES,
  MANUFACTURING_RD_MATERIAL_RESERVATIONS,
  MANUFACTURING_RD_ORDER_POSTINGS,
  MANUFACTURING_RD_ORDER_OPERATIONS,
  MANUFACTURING_RD_LOT_GENEALOGY,
} from '../../../packages/server-manufacturing/src/adapters/native-manufacturing-read';

const CONFIG_KEY = 'ERPX_NATIVE_MANUFACTURING_READ_CONFIG';
const FALLBACK_KEY = 'ERPX_NATIVE_MANUFACTURING_CONFIG';
const originals = new Map([CONFIG_KEY, FALLBACK_KEY].map((key) => [key, process.env[key]]));
const ok = (value: unknown) => new Response(JSON.stringify(value), {
  status: 200,
  headers: { 'content-type': 'application/json' },
});

const statement = { sql: 'SELECT o.* FROM manufacturing_orders o WHERE o.company_id=?', values: ['company-1'] };

describe('native Manufacturing read-plan transport', () => {
  beforeEach(() => {
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44994',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
    });
  });
  afterEach(() => {
    for (const [key, value] of originals) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    vi.unstubAllGlobals();
  });

  it('posts the closed kind and bounded scope to the exact Holding route', async () => {
    let url = '';
    let init: RequestInit | undefined;
    vi.stubGlobal('fetch', vi.fn(async (u: URL, i: RequestInit) => {
      url = String(u);
      init = i;
      return ok({ data: { kind: MANUFACTURING_RD_ORDERS, caller: 'manufacturing.workspace', statement } });
    }));
    const planned = await nativeManufacturingPlan(MANUFACTURING_RD_ORDERS, {
      companyId: 'company-1', branchId: undefined,
    });
    expect(planned.sql).toBe(statement.sql);
    expect(planned.values).toEqual(['company-1']);
    expect(url).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/manufacturing/read-plan');
    expect(init?.method).toBe('POST');
    expect((init?.headers as Headers).get('host')).toBe('acme.example.com');
    expect((init?.headers as Headers).get('origin')).toBe('https://acme.example.com');
    expect(JSON.parse(String(init?.body))).toEqual({
      kind: MANUFACTURING_RD_ORDERS,
      scope: { companyId: 'company-1' },
    });
  });

  it('executes the owner statement on the caller unit, not on the transport', async () => {
    const prepares: string[] = [];
    const binds: unknown[][] = [];
    const db = {
      prepare(sql: string) {
        prepares.push(sql);
        return {
          bind(...values: unknown[]) {
            binds.push(values);
            return { /* marker for batch */ sql, values };
          },
        };
      },
      batch: async () => [{ results: [{ id: 'order-1' }], success: true }],
    } as never;
    vi.stubGlobal('fetch', vi.fn(async () => ok({
      data: { kind: MANUFACTURING_RD_ORDERS, caller: 'manufacturing.workspace', statement },
    })));
    const result = await nativeManufacturingReadAll<{ id: string }>(MANUFACTURING_RD_ORDERS, {
      companyId: 'company-1',
    }, db);
    expect(result.results).toEqual([{ id: 'order-1' }]);
    expect(prepares).toEqual([statement.sql]);
    expect(binds).toEqual([['company-1']]);
  });

  it('fails closed for unknown kinds, absent config and rejected or malformed responses', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(nativeManufacturingPlan('bogus.read')).rejects.toMatchObject({
      code: 'MANUFACTURING_READ_PLAN_INVALID',
    });
    await expect(nativeManufacturingPlan(MANUFACTURING_RD_SPECIFICATION, { id: 'x'.repeat(201) }))
      .rejects.toMatchObject({ code: 'MANUFACTURING_READ_PLAN_INVALID' });
    delete process.env[CONFIG_KEY];
    delete process.env[FALLBACK_KEY];
    await expect(nativeManufacturingPlan(MANUFACTURING_RD_SPECIFICATION))
      .rejects.toMatchObject({ code: 'MANUFACTURING_NATIVE_READ_UNAVAILABLE', status: 503 });
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44994',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme' }],
    });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      error: { code: 'ORIGIN_FORBIDDEN', message: 'Use the registered application origin.' },
    }), { status: 403 })));
    await expect(nativeManufacturingPlan(MANUFACTURING_RD_ORDER_EVENTS))
      .rejects.toMatchObject({ code: 'ORIGIN_FORBIDDEN', status: 403 });
    vi.stubGlobal('fetch', vi.fn(async () => ok({
      data: { kind: MANUFACTURING_RD_ORDER_EVENTS, caller: 'records.detail',
        statement: { sql: 'DROP TABLE manufacturing_orders', values: [] } },
    })));
    await expect(nativeManufacturingPlan(MANUFACTURING_RD_ORDER_EVENTS))
      .rejects.toMatchObject({ code: 'MANUFACTURING_NATIVE_READ_UNAVAILABLE' });
  });

  it('admits the five r22 detail kinds and forwards their exact bounded scope', async () => {
    const cases = [
      [MANUFACTURING_RD_ORDER_SPECIFICATION_LINES, { specificationId: 'spec-1' },
        'SELECT l.* FROM manufacturing_specification_lines l WHERE l.specification_id=?', ['spec-1']],
      [MANUFACTURING_RD_MATERIAL_RESERVATIONS, { orderId: 'order-1' },
        'SELECT r.* FROM manufacturing_material_reservations r WHERE r.order_id=?', ['order-1']],
      [MANUFACTURING_RD_ORDER_POSTINGS, { orderId: 'order-1' },
        'SELECT p.* FROM manufacturing_order_postings p WHERE p.order_id=?', ['order-1']],
      [MANUFACTURING_RD_ORDER_OPERATIONS, { orderId: 'order-1', specificationId: 'spec-1' },
        'SELECT op.* FROM manufacturing_specification_operations op WHERE e.order_id=? AND op.specification_id=?', ['order-1', 'spec-1']],
      [MANUFACTURING_RD_LOT_GENEALOGY, { orderId: 'order-1' },
        'SELECT g.* FROM manufacturing_lot_genealogy g WHERE g.order_id=?', ['order-1']],
    ] as const;
    for (const [kind, scope, sql, values] of cases) {
      let body: unknown;
      vi.stubGlobal('fetch', vi.fn(async (_u: URL, i: RequestInit) => {
        body = JSON.parse(String(i.body));
        return ok({ data: { kind, caller: 'records.detail', statement: { sql, values } } });
      }));
      const planned = await nativeManufacturingPlan(kind, scope);
      expect(planned.sql, kind).toBe(sql);
      expect(planned.values, kind).toEqual(values);
      expect(body, kind).toEqual({ kind, scope });
    }
  });

  it('rejects malformed configuration before any fetch', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3100', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme' }],
    });
    await expect(nativeManufacturingPlan(MANUFACTURING_RD_SPECIFICATION))
      .rejects.toMatchObject({ code: 'MANUFACTURING_NATIVE_READ_CONFIG_INVALID' });
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000,
      holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme' }],
    });
    await expect(nativeManufacturingPlan(MANUFACTURING_RD_SPECIFICATION))
      .rejects.toMatchObject({ code: 'MANUFACTURING_NATIVE_READ_CONFIG_INVALID' });
  });
});
