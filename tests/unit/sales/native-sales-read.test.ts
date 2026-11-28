import { createServer, type Server } from 'node:http';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  SALES_RD_NOTIFICATION_ATTEMPT,
  SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED,
  SALES_RD_PRICE_LIST_ACTIVE,
  SALES_RD_PRICE_LIST_ITEMS_ACTIVE,
  nativeSalesReadAll,
  nativeSalesReadFirst,
} from '../../../modules/sales/server/src/adapters/native-sales-read';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

const host = 'acme.example.com';
const origin = `https://${host}`;
const company = 'company';

const config = (privateBaseUrl: string) =>
  JSON.stringify({
    privateBaseUrl,
    timeoutMs: 1000,
    holdings: [{ origin, holdingSlug: 'acme', holdingId: 'holding' }],
  });

let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  delete process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG;
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
    async batch<T>(statements: readonly { sql: string; values: unknown[] }[]) {
      return statements.map(() => ({ results: [row] as T[], success: true, meta: { changes: 0, duration: 0 } }));
    },
  };
  return db as unknown as SqlDatabase;
}

describe('native Sales command-plan read transport', () => {
  it('fails closed when the native owner transport is not configured', async () => {
    delete process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG;
    await expect(
      nativeSalesReadAll(SALES_RD_PRICE_LIST_ACTIVE, [company], database([])),
    ).rejects.toMatchObject({ code: 'SALES_NATIVE_READ_UNAVAILABLE', status: 503 });
  });

  it('fails closed on a malformed native owner transport configuration', async () => {
    process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG = config('http://127.0.0.1:3100');
    await expect(
      nativeSalesReadAll(SALES_RD_PRICE_LIST_ACTIVE, [company], database([])),
    ).rejects.toBeInstanceOf(Error);
  });

  it('sends only kind and values, then runs the owner-planned statement on the caller unit', async () => {
    const sql = "SELECT id,name,customer_id,valid_from,valid_until,max_auto_discount_bps,version FROM sales_price_lists WHERE company_id=? AND status='active' ORDER BY customer_id NULLS LAST,name";
    let request: { headers: Record<string, unknown>; url?: string; body: string } | undefined;
    const base = await listen(req => {
      request = req;
      return { status: 200, value: { data: { statements: [{ sql, values: [company] }] } } };
    });
    process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG = config(base);
    const calls: Call[] = [];
    const row = { id: 'list' };
    await expect(
      nativeSalesReadAll(SALES_RD_PRICE_LIST_ACTIVE, [company], database(calls, row)),
    ).resolves.toMatchObject({ results: [row] });
    expect(JSON.parse(request!.body)).toMatchObject({
      kind: SALES_RD_PRICE_LIST_ACTIVE,
      values: [company],
    });
    expect(request!.headers).toMatchObject({
      host,
      origin,
      accept: 'application/json',
      'content-type': 'application/json',
    });
    expect(request!.url).toBe('/api/v1/holdings/acme/sales/command-plan');
    expect(calls).toEqual([{ sql, values: [company], first: false }]);
  });

  it('supports the price-list items read and single-row parity on the caller unit', async () => {
    const sql = "SELECT i.* FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id WHERE l.company_id=? AND l.status='active' ORDER BY i.price_list_id,i.ordinal";
    const base = await listen(() => ({
      status: 200,
      value: { data: { statements: [{ sql, values: [company] }] } },
    }));
    process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG = config(base);
    const calls: Call[] = [];
    await expect(
      nativeSalesReadFirst(SALES_RD_PRICE_LIST_ITEMS_ACTIVE, [company], database(calls)),
    ).resolves.toEqual({ one: 1 });
    // The owner-planned statement is prepared on the caller unit and run by batch.
    expect(calls).toEqual([{ sql, values: [company], first: false }]);
  });

  it('supports the three-bind notification attempt read on the caller unit', async () => {
    const sql = "SELECT attempt.id,attempt.source_hash,attempt.provider_message_id,authority.actor_id FROM sales_notification_execution_attempts attempt JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id AND authority.company_id=attempt.company_id AND authority.notification_id=attempt.notification_id WHERE attempt.notification_id=? AND attempt.company_id=? AND attempt.attempt=? AND attempt.state='unknown'";
    let request: { body: string } | undefined;
    const base = await listen(req => {
      request = req;
      return { status: 200, value: { data: { statements: [{ sql, values: ['notice', company, 2] }] } } };
    });
    process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG = config(base);
    const calls: Call[] = [];
    await expect(
      nativeSalesReadFirst(SALES_RD_NOTIFICATION_ATTEMPT, ['notice', company, 2], database(calls)),
    ).resolves.toEqual({ one: 1 });
    expect(JSON.parse(request!.body)).toMatchObject({
      kind: SALES_RD_NOTIFICATION_ATTEMPT,
      values: ['notice', company, 2],
    });
    expect(calls).toEqual([{ sql, values: ['notice', company, 2], first: false }]);
  });

  it('supports the zero-bind reconciliation-installed read on the caller unit', async () => {
    const sql = "SELECT to_regclass('sales_notification_reconciliations') IS NOT NULL installed";
    let request: { body: string } | undefined;
    const base = await listen(req => {
      request = req;
      return { status: 200, value: { data: { statements: [{ sql, values: [] }] } } };
    });
    process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG = config(base);
    const calls: Call[] = [];
    await expect(
      nativeSalesReadFirst<{ installed: boolean }>(
        SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED,
        [],
        database(calls, { installed: true }),
      ),
    ).resolves.toEqual({ installed: true });
    expect(JSON.parse(request!.body)).toMatchObject({
      kind: SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED,
      values: [],
    });
    expect(calls).toEqual([{ sql, values: [], first: false }]);
  });

  it('fails closed on a malformed native plan reply', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { statements: [] } } }));
    process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG = config(base);
    await expect(
      nativeSalesReadAll(SALES_RD_PRICE_LIST_ACTIVE, [company], database([])),
    ).rejects.toMatchObject({ code: 'SALES_NATIVE_READ_UNAVAILABLE', status: 503 });
  });
});
