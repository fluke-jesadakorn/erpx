import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  CA_RD_ACCESS_EVENTS,
  CA_RD_ACCESS_IDENTITIES,
  CA_RD_ACCESS_READS,
  CA_RD_DUPLICATE_REVIEW_HISTORY,
  CA_RD_DUPLICATE_REVIEWS_PENDING,
  CA_RD_IDENTITY_CHANGE_HISTORY,
  nativeOwnerReadAll,
  nativeOwnerReadStatement,
} from '../../../packages/server-customer_authority/src/adapters/native-owner-read';

const key = 'ERPX_NATIVE_CUSTOMER_AUTHORITY_READ_CONFIG';
const config = () => JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44992',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', companyId: 'company' }],
});
const fetchPlan = (sql: string, values: unknown[]) => vi.fn(async () => new Response(
  JSON.stringify({ data: { statements: [{ sql, values }] } }),
  { status: 200, headers: { 'content-type': 'application/json' } },
));

describe('native Customer Authority change-history owner reads', () => {
  it('plans the owner statement and executes it on the caller unit', async () => {
    process.env[key] = config();
    const sql = 'SELECT * FROM customer_identity_change_events WHERE change_id=? ORDER BY version';
    vi.stubGlobal('fetch', fetchPlan(sql, ['change']));
    const seen: { sql: string; values: unknown[] }[] = [];
    const db = {
      prepare: (statement: string) => ({
        bind: (...values: unknown[]) => {
          seen.push({ sql: statement, values });
          return { all: async () => ({ results: [{ version: 1 }] }) };
        },
      }),
    };
    const result = await nativeOwnerReadAll(CA_RD_IDENTITY_CHANGE_HISTORY, ['change'], db as never, 'acme');
    expect(result.results).toEqual([{ version: 1 }]);
    expect(seen).toEqual([{ sql, values: ['change'] }]);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('fails closed when the native owner read service is unconfigured', async () => {
    delete process.env[key];
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(nativeOwnerReadAll(
      CA_RD_DUPLICATE_REVIEW_HISTORY,
      ['review'],
      { prepare: () => { throw new Error('must not prepare'); } } as never,
      'acme',
    )).rejects.toMatchObject({ code: 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', status: 503 });
    vi.unstubAllGlobals();
  });

  it('carries the exact owner discriminator for both history kinds', () => {
    expect(CA_RD_IDENTITY_CHANGE_HISTORY).toBe('customer_authority_rd_identity_change_history');
    expect(CA_RD_DUPLICATE_REVIEW_HISTORY).toBe('customer_authority_rd_duplicate_review_history');
  });

  it('plans customer-access history reads on the caller unit', async () => {
    process.env[key] = config();
    for (const [kind, table] of [
      [CA_RD_ACCESS_EVENTS, 'customer_access_events'],
      [CA_RD_ACCESS_READS, 'customer_access_reads'],
    ] as const) {
      const sql = `SELECT * FROM ${table} WHERE binding_id=?`;
      vi.stubGlobal('fetch', fetchPlan(sql, ['binding']));
      const seen: { sql: string; values: unknown[] }[] = [];
      const db = {
        prepare: (statement: string) => ({
          bind: (...values: unknown[]) => {
            seen.push({ sql: statement, values });
            return { all: async () => ({ results: [{ version: 1 }] }) };
          },
        }),
      };
      const result = await nativeOwnerReadAll(kind, ['binding'], db as never, 'acme');
      expect(result.results).toEqual([{ version: 1 }]);
      expect(seen).toEqual([{ sql, values: ['binding'] }]);
    }
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('carries the exact customer-access history discriminators', () => {
    expect(CA_RD_ACCESS_EVENTS).toBe('customer_authority_rd_access_events');
    expect(CA_RD_ACCESS_READS).toBe('customer_authority_rd_access_reads');
  });

  it('plans the bound-identity existence read on the caller unit', async () => {
    process.env[key] = config();
    const sql = 'SELECT 1 FROM customer_access_identities WHERE binding_id=?';
    vi.stubGlobal('fetch', fetchPlan(sql, ['binding']));
    const seen: { sql: string; values: unknown[] }[] = [];
    const db = {
      prepare: (statement: string) => ({
        bind: (...values: unknown[]) => {
          seen.push({ sql: statement, values });
          return { all: async () => ({ results: [{ '1': 1 }] }) };
        },
      }),
    };
    const result = await nativeOwnerReadAll(CA_RD_ACCESS_IDENTITIES, ['binding'], db as never, 'acme');
    expect(result.results).toEqual([{ '1': 1 }]);
    expect(seen).toEqual([{ sql, values: ['binding'] }]);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('carries the exact bound-identity discriminator', () => {
    expect(CA_RD_ACCESS_IDENTITIES).toBe('customer_authority_rd_access_identities');
  });

  it('plans the pending duplicate-review metric count without executing it', async () => {
    process.env[key] = config();
    const sql = "SELECT 'Pending reviews' label,count(*)::text value FROM customer_duplicate_reviews WHERE company_id=? AND status='pending'";
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { statements: [{ sql, values: ['company'] }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    const statement = await nativeOwnerReadStatement(
      CA_RD_DUPLICATE_REVIEWS_PENDING, ['company'], 'acme',
    );
    expect(statement).toEqual({ sql, values: ['company'] });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const body = JSON.parse(((fetchMock.mock.calls[0] as unknown as [URL, RequestInit])[1] as RequestInit).body as string);
    expect(body.kind).toBe('customer_authority_rd_duplicate_reviews_pending');
    expect(body.values).toEqual(['company']);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('carries the exact pending duplicate-review metric discriminator', () => {
    expect(CA_RD_DUPLICATE_REVIEWS_PENDING).toBe('customer_authority_rd_duplicate_reviews_pending');
  });
});
