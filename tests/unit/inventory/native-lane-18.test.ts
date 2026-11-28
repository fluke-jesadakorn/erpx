import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
const nativeRequestMock=vi.hoisted(()=>vi.fn());
vi.mock('@erpx/platform-runtime-server/public/native-http',()=>({nativeRequest:nativeRequestMock}));
const incomingHost = { value: 'acme.jesadakorn.com' as string | null };
vi.mock('next/headers', () => ({ headers: async () => new Headers(incomingHost.value === null ? {} : { host: incomingHost.value }) }));
import { PosReadInventoryFacts } from '../../../modules/inventory/server/src/adapters/pos-facts';
import { readStockAvailability, type Lane18Actor } from '../../../modules/inventory/server/src/adapters/native-lane-18/client';

const hash = (value: string) => createHash('sha256').update(value).digest('hex');
const SQL_ORACLE = 'fbb602ffe25f6c0aafacba7fb88c8eb90dad2885f3aec75ccd7f43d6b7593b10';
const RESULT_ORACLE = '77aaeea853b67153f2d4040897a2d3e47a21f8c48e2a63658ac90a1b90c77437';
const OWNER_SELECT_PIN = 276;
const envKey = 'ERPX_NATIVE_INVENTORY_LANE18_READ_CONFIG';
const original = process.env[envKey];

const DEV_ENV_JSON = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co' },
    { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar', companyId: 'company-northstar' },
    { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'erpx-dev' },
  ],
} as const;

const actor = (holdingId: string, holdingSlug: string, companyId: string): Lane18Actor => ({ holdingId, holdingSlug, companyId });
const acme = actor('h001', 'acme', 'acme-co');
const northstar = actor('h002', 'northstar', 'company-northstar');

const sampleRows = [
  { warehouse_id: 'w1', product_id: 'p1', quantity_milli: 1200, reserved_milli: 200, quality_held: 0 },
  { warehouse_id: 'w1', product_id: 'p2', quantity_milli: 500, reserved_milli: 0, quality_held: 1 },
];
const samplePayload = { rows: sampleRows, count: 2 };

function configure() {
  process.env[envKey] = JSON.stringify(DEV_ENV_JSON);
}

function stubTransport(status: number, payload: unknown) {
  const calls: { url: URL; headers: Record<string, string> }[] = [];
  nativeRequestMock.mockImplementation(async (url: URL, init: {headers: Record<string,string>}) => {
    calls.push({ url, headers: init.headers });
    return new Response(JSON.stringify(payload), { status, headers: { 'content-type': 'application/json' } });
  });
  return calls;
}

describe('native Inventory lane-18 stock-availability contract', () => {
  beforeEach(() => { incomingHost.value = 'acme.jesadakorn.com'; configure(); });
  afterEach(() => {
    if (original === undefined) delete process.env[envKey];
    else process.env[envKey] = original;
    nativeRequestMock.mockReset();
  });

  it('retired the TypeScript SQL with no fallback', () => {
    const source = readFileSync(resolve(process.cwd(), 'modules/inventory/server/src/adapters/pos-facts.ts'), 'utf8');
    expect(source).not.toMatch(/SELECT\s/);
    expect(source).not.toContain('all(db');
    expect(source).toContain('readStockAvailability');
  });

  it('pins the retired SQL, a canonical result and the final owner SELECT KPI', () => {
    const source = readFileSync(resolve(process.cwd(), 'modules/inventory/server/src/adapters/native_lane_18/postgres.rs'), 'utf8');
    const match = source.match(/pub const INNER_SQL: &str = r#"([\s\S]*?)"#;/);
    expect(match).not.toBeNull();
    expect(hash(match![1])).toBe(SQL_ORACLE);
    expect(match![1]).toContain('w.company_id=$1');
    expect(match![1]).toContain('inventory_active_reservations');
    expect(hash(JSON.stringify(samplePayload))).toBe(RESULT_ORACLE);
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(process.cwd(), 'modules/inventory/server/src'));
    expect(total).toBe(OWNER_SELECT_PIN);
  });

  it('selects the Holding from WorkContext and pins the response shape', async () => {
    const calls = stubTransport(200, samplePayload);
    const db = { prepare() { throw new Error('no SQL fallback'); } };
    const facts = PosReadInventoryFacts(db as never, acme);
    const rows = await facts.readInventoryActiveReservations1({ company: 'acme-co', branch: 'branch-2', branch2: 'branch-2' });
    expect(rows).toEqual(sampleRows);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url.pathname).toBe('/api/v1/holdings/acme/inventory/lane-18/stock-availability');
    expect(calls[0]!.headers['host']).toBe('acme.jesadakorn.com');
    expect(calls[0]!.headers['x-erpx-context-company']).toBe('acme-co');
    expect(calls[0]!.headers['x-erpx-context-holding']).toBe('h001');
    expect(calls[0]!.headers['x-erpx-context-branch']).toBe('branch-2');
  });

  it('routes a different registered Holding to its own host, slug and company', async () => {
    const calls = stubTransport(200, { rows: [], count: 0 });
    incomingHost.value = 'northstar.jesadakorn.com';
    await readStockAvailability(northstar, 'company-northstar', null);
    expect(calls[0]!.url.pathname).toBe('/api/v1/holdings/northstar/inventory/lane-18/stock-availability');
    expect(calls[0]!.headers['host']).toBe('northstar.jesadakorn.com');
    expect(calls[0]!.headers['x-erpx-context-company']).toBe('company-northstar');
    expect(calls[0]!.headers).not.toHaveProperty('x-erpx-context-branch');
  });

  it('has no first-Holding default: unknown or mismatched WorkContext fails closed', async () => {
    const calls = stubTransport(200, samplePayload);
    await expect(readStockAvailability(actor('h999', 'acme', 'acme-co'), 'acme-co', null)).rejects.toMatchObject({ status: 409, code: 'INVENTORY_LANE18_WORK_CONTEXT_CHANGED' });
    await expect(readStockAvailability(actor('h002', 'northstar', 'acme-co'), 'acme-co', null)).rejects.toMatchObject({ status: 409 });
    await expect(readStockAvailability(acme, 'company-northstar', null)).rejects.toMatchObject({ status: 409 });
    expect(calls).toHaveLength(0);
  });

  it('pins a legacy fact caller to the exact incoming Host', async () => {
    const calls = stubTransport(200, { rows: [], count: 0 });
    incomingHost.value = 'northstar.jesadakorn.com';
    await readStockAvailability(undefined, 'company-northstar', null);
    expect(calls[0]!.url.pathname).toBe('/api/v1/holdings/northstar/inventory/lane-18/stock-availability');
    incomingHost.value = 'unknown.example.com';
    await expect(readStockAvailability(undefined, 'company-northstar', null)).rejects.toMatchObject({ status: 409 });
    expect(calls).toHaveLength(1);
  });

  it('fails closed when the native route is unavailable or the payload drifts', async () => {
    stubTransport(503, 'down');
    await expect(readStockAvailability(acme, 'acme-co', null)).rejects.toMatchObject({ status: 503, code: 'INVENTORY_LANE18_READ_UNAVAILABLE' });
    stubTransport(200, { rows: [{ warehouse_id: 'w1' }], count: 1 });
    await expect(readStockAvailability(acme, 'acme-co', null)).rejects.toMatchObject({ status: 503 });
    delete process.env[envKey];
    await expect(readStockAvailability(acme, 'acme-co', null)).rejects.toMatchObject({ status: 503 });
  });
});
