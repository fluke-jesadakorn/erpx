import { createServer, type Server } from 'node:http';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  nativeCommerceReadAll,
  nativeCommerceReadFirst,
  NativeCommerceReadError,
} from '../../../packages/server-commerce/src/adapters/native-commerce-read';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const host = 'acme.example.com';
const origin = `https://${host}`;
const company = 'company';

const config = (privateBaseUrl: string) =>
  JSON.stringify({ privateBaseUrl, host, holdingSlug: 'acme', origin, timeoutMs: 1000 });

let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  delete process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
});

async function listen(
  handler: (req: { headers: Record<string, unknown>; url?: string; body: string }) => {
    status: number;
    value: unknown;
  },
): Promise<string> {
  server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on('data', chunk => chunks.push(Buffer.from(chunk)));
    req.on('end', () => {
      const result = handler({
        headers: req.headers,
        url: req.url,
        body: Buffer.concat(chunks).toString('utf8'),
      });
      res.writeHead(result.status, { 'content-type': 'application/json' });
      res.end(JSON.stringify(result.value));
    });
  });
  await new Promise<void>(resolve => server!.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no listener');
  return `http://127.0.0.1:${address.port}`;
}

type Call = { sql: string; values: unknown[]; first: boolean };
/** Minimal caller unit: records the owner-planned statement it is asked to run. */
function database(calls: Call[], row: unknown = { one: 1 }): SqlDatabase {
  const db: Record<string, unknown> = {
    prepare(sql: string) {
      const call: Call = { sql, values: [], first: false };
      calls.push(call);
      return {
        bind(...values: unknown[]) {
          call.values = values;
          return this;
        },
        async first<T>() {
          call.first = true;
          return row as T;
        },
        async all<T>() {
          return { results: [row] as T[] };
        },
      };
    },
  };
  return db as unknown as SqlDatabase;
}

describe('native Commerce command-plan read transport', () => {
  it('fails closed when the native owner transport is not configured', async () => {
    delete process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
    await expect(
      nativeCommerceReadFirst('commerce_rd_pms_warehouse_active', [company], database([])),
    ).rejects.toMatchObject({ code: 'COMMERCE_READ_UNAVAILABLE', status: 503 });
  });

  it('fails closed on a malformed native owner transport configuration', async () => {
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config('http://127.0.0.1:3100');
    await expect(
      nativeCommerceReadFirst('commerce_rd_pms_settings_exists', [company], database([])),
    ).rejects.toBeInstanceOf(NativeCommerceReadError);
  });

  it('sends only kind and values, then runs the owner-planned statement on the caller unit', async () => {
    const sql = "SELECT 1 FROM commerce_warehouses WHERE company_id=? AND status='active' LIMIT 1";
    let request: { headers: Record<string, unknown>; url?: string; body: string } | undefined;
    const base = await listen(req => {
      request = req;
      return { status: 200, value: { data: { statements: [{ sql, values: [company] }] } } };
    });
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config(base);
    const calls: Call[] = [];
    const row = { one: 1 };
    await expect(
      nativeCommerceReadFirst('commerce_rd_pms_warehouse_active', [company], database(calls, row)),
    ).resolves.toEqual(row);
    // The wire carries no SQL from the TS side.
    expect(JSON.parse(request!.body)).toEqual({
      kind: 'commerce_rd_pms_warehouse_active',
      values: [company],
    });
    expect(request!.headers).toMatchObject({
      host,
      origin,
      accept: 'application/json',
      'content-type': 'application/json',
    });
    expect(request!.url).toBe('/api/v1/holdings/acme/commerce/command-plan');
    // The owner-planned statement and its binds are executed on the caller unit.
    expect(calls).toEqual([{ sql, values: [company], first: true }]);
  });

  it('supports all-row reads on the caller unit', async () => {
    const sql = 'SELECT 1 FROM commerce_settings WHERE company_id=?';
    const base = await listen(() => ({
      status: 200,
      value: { data: { statements: [{ sql, values: [company] }] } },
    }));
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config(base);
    const calls: Call[] = [];
    await expect(
      nativeCommerceReadAll('commerce_rd_pms_settings_exists', [company], database(calls)),
    ).resolves.toEqual([{ one: 1 }]);
    expect(calls).toEqual([{ sql, values: [company], first: false }]);
  });

  it('fails closed on a malformed native plan reply', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { statements: [] } } }));
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config(base);
    await expect(
      nativeCommerceReadFirst('commerce_rd_pms_settings_exists', [company], database([])),
    ).rejects.toMatchObject({ code: 'COMMERCE_READ_RESPONSE_INVALID', status: 503 });
  });
});
