import { createServer, type Server } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  nativeManufacturingLane28StockHistory,
  nativeManufacturingLane28PostingReferences,
  nativeManufacturingLane28Configured,
} from '../../../packages/server-manufacturing/src/adapters/native-lane-28/client';

const host = 'acme.example.test';
const origin = `https://${host}`;
const company = 'acme-co';
const warehouse = 'wh';
const product = 'product';
const ENV = 'ERPX_NATIVE_MANUFACTURING_LANE28_CONFIG';
const path = '/api/v1/holdings/acme/manufacturing/lane28/initial-stock-history';
const postingsPath = '/api/v1/holdings/acme/manufacturing/lane28/existing-posting-references';

const config = (base: string) =>
  JSON.stringify({
    privateBaseUrl: base,
    timeoutMs: 1000,
    holdings: [
      { origin, host, holdingId: 'h001', holdingSlug: 'acme', companyId: company },
      {
        origin: 'https://northstar.example.test',
        host: 'northstar.example.test',
        holdingId: 'h002',
        holdingSlug: 'northstar',
        companyId: 'northstar-co',
      },
      {
        origin: 'https://erpx-internal.jesadakorn.com',
        host: 'erpx-internal.jesadakorn.com',
        holdingId: 'h003',
        holdingSlug: 'erpx',
      },
    ],
  });

let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  delete process.env[ENV];
});

async function listen(
  handler: (req: { headers: Record<string, unknown>; url?: string }) => { status: number; value: unknown },
): Promise<string> {
  server = createServer((req, res) => {
    const result = handler({ headers: req.headers, url: req.url });
    res.writeHead(result.status, { 'content-type': 'application/json' });
    res.end(JSON.stringify(result.value));
  });
  await new Promise<void>((resolve) => server!.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no listener');
  return `http://127.0.0.1:${address.port}`;
}

describe('native Manufacturing lane-28 stock-history transport', () => {
  it('fails closed when the native owner transport is not configured', async () => {
    delete process.env[ENV];
    expect(nativeManufacturingLane28Configured()).toBe(false);
    await expect(
      nativeManufacturingLane28StockHistory(company, warehouse, product),
    ).rejects.toMatchObject({ status: 503, code: 'MANUFACTURING_LANE28_CONFIGURATION_REQUIRED' });
  });

  it('fails closed on a malformed native owner configuration', async () => {
    process.env[ENV] = config('http://127.0.0.1:3100');
    await expect(
      nativeManufacturingLane28StockHistory(company, warehouse, product),
    ).rejects.toMatchObject({ status: 503, code: 'MANUFACTURING_LANE28_CONFIGURATION_INVALID' });
  });

  it('pins the boolean fact, scoped headers and no SQL on the wire', async () => {
    let request: { headers: Record<string, unknown>; url?: string } | undefined;
    const base = await listen((req) => {
      request = req;
      return { status: 200, value: { data: { history: true } } };
    });
    process.env[ENV] = config(base);
    await expect(
      nativeManufacturingLane28StockHistory(company, warehouse, product),
    ).resolves.toBe(true);
    expect(request?.url).toBe(path);
    expect(request?.headers).toMatchObject({
      origin,
      accept: 'application/json',
      'x-erpx-context-company': company,
      'x-erpx-context-warehouse': warehouse,
      'x-erpx-context-product': product,
    });
    expect(JSON.stringify(request)).not.toContain('SELECT');
  });

  it('selects the Holding from the caller WorkContext, not the first holding', async () => {
    let request: { headers: Record<string, unknown>; url?: string } | undefined;
    const base = await listen((req) => {
      request = req;
      return { status: 200, value: { data: { history: false } } };
    });
    process.env[ENV] = config(base);
    await expect(
      nativeManufacturingLane28StockHistory('northstar-co', warehouse, product, {
        holdingId: 'h002',
        holdingSlug: 'northstar',
        companyId: 'northstar-co',
      }),
    ).resolves.toBe(false);
    expect(request?.url).toBe('/api/v1/holdings/northstar/manufacturing/lane28/initial-stock-history');
    expect(request?.headers).toMatchObject({ 'x-erpx-context-company': 'northstar-co' });
  });

  it('fails closed when the WorkContext Holding is not registered', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { history: true } } }));
    process.env[ENV] = config(base);
    await expect(
      nativeManufacturingLane28StockHistory(company, warehouse, product, {
        holdingId: 'h9xx',
        holdingSlug: 'foreign',
        companyId: company,
      }),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
  });

  it('has no first-holding default', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { history: true } } }));
    process.env[ENV] = config(base);
    await expect(
      nativeManufacturingLane28StockHistory('unknown-company', warehouse, product),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
  });

  it('fails closed on a malformed native payload', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { history: 'true' } } }));
    process.env[ENV] = config(base);
    await expect(
      nativeManufacturingLane28StockHistory(company, warehouse, product),
    ).rejects.toMatchObject({ status: 503, code: 'MANUFACTURING_LANE28_RESPONSE_INVALID' });
  });

  it('fails closed when the native owner is unavailable', async () => {
    const base = await listen(() => ({ status: 503, value: { error: { code: 'X', message: 'down' } } }));
    process.env[ENV] = config(base);
    await expect(
      nativeManufacturingLane28StockHistory(company, warehouse, product),
    ).rejects.toMatchObject({ status: 503 });
  });
});

describe('native Manufacturing lane-28 posting-references transport', () => {
  it('pins the references fact, scoped header and no SQL on the wire', async () => {
    let request: { headers: Record<string, unknown>; url?: string } | undefined;
    const base = await listen((req) => {
      request = req;
      return {
        status: 200,
        value: { data: { journalIds: ['j1', 'j2'], orderIds: ['o1'], invalid: true, image: '[]' } },
      };
    });
    process.env[ENV] = config(base);
    await expect(nativeManufacturingLane28PostingReferences(company)).resolves.toEqual({
      journalIds: ['j1', 'j2'],
      orderIds: ['o1'],
      invalid: true,
      image: '[]',
    });
    expect(request?.url).toBe(postingsPath);
    expect(request?.headers).toMatchObject({
      origin,
      accept: 'application/json',
      'x-erpx-context-company': company,
    });
    expect(JSON.stringify(request)).not.toContain('SELECT');
  });

  it('fails closed on a malformed references payload', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { journalIds: ['j1'], orderIds: [], invalid: 'true', image: '[]' } } }));
    process.env[ENV] = config(base);
    await expect(nativeManufacturingLane28PostingReferences(company)).rejects.toMatchObject({
      status: 503,
      code: 'MANUFACTURING_LANE28_RESPONSE_INVALID',
    });
  });

  it('rejects an empty references identity before any call', async () => {
    const base = await listen(() => ({ status: 200, value: { data: {} } }));
    process.env[ENV] = config(base);
    await expect(nativeManufacturingLane28PostingReferences('')).rejects.toMatchObject({
      status: 503,
      code: 'MANUFACTURING_LANE28_COMPANY_INVALID',
    });
  });
});

const guard = (table: string, alias: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} JOIN manufacturing_orders o ON o.id=${alias}.order_id WHERE o.company_id=$1 AND ${alias}.warehouse_id=$2 AND ${alias}.product_id=$3)`;
const sql = `SELECT ${guard('manufacturing_material_reservations', 'r')} OR ${guard('manufacturing_inventory_moves', 'm')} AS history`;
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const fnv = (value: string) => {
  let hash = 0xcbf29ce484222325n;
  for (const byte of Buffer.from(value)) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return `0x${hash.toString(16)}`;
};

describe('native lane-28 stock-history SQL oracle', () => {
  it('pins rendered SQL hashes shared by Rust and TypeScript', () => {
    expect((sql.match(/SELECT /g) ?? []).length).toBe(3);
    expect(sha(sql)).toBe('f0c4bf5922861bd1152a965183c89f9175125d30af2eab2c3c600ebd02c289cf');
    expect(fnv(sql)).toBe('0x374c62bc1d3d2304');
  });

  it('pins the Rust owner constants and leaves no SQL in the replaced adapter', async () => {
    const domain = await readFile(
      new URL('../../../packages/server-manufacturing/src/adapters/native_lane_28/domain.rs', import.meta.url),
      'utf8',
    );
    expect(domain).toContain('f0c4bf5922861bd1152a965183c89f9175125d30af2eab2c3c600ebd02c289cf');
    expect(domain).toContain('0x374c62bc1d3d2304');
    const adapter = await readFile(
      new URL('../../../packages/server-manufacturing/src/adapters/initial-physical-stock-sources.ts', import.meta.url),
      'utf8',
    );
    expect(adapter).not.toMatch(/SELECT|prepare\([^']|manufacturingExistsSelect/);
    expect(adapter).toContain('nativeManufacturingLane28StockHistory');
  });
});
