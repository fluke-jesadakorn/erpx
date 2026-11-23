import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  CA_RD_DUPLICATE_REVIEW_HISTORY,
  CA_RD_IDENTITY_CHANGE_HISTORY,
  nativeOwnerReadAll,
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
});
