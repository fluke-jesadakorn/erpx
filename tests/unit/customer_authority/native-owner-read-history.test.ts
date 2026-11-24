import { describe, it, expect, vi } from 'vitest';
import { readFile } from 'node:fs/promises';
vi.mock('server-only', () => ({}));
import {
  CA_RD_ACCESS_EVENTS,
  CA_RD_ACCESS_IDENTITIES,
  CA_RD_ACCESS_READS,
  CA_RD_DUPLICATE_REVIEW_HISTORY,
  CA_RD_DUPLICATE_REVIEWS_PENDING,
  CA_RD_IDENTITY_CHANGE_EVENTS_ACTION,
  CA_RD_IDENTITY_CHANGE_EVENTS_MONTH,
  CA_RD_IDENTITY_CHANGE_HISTORY,
  nativeOwnerReadAll,
  nativeOwnerReadStatement,
} from '../../../packages/server-customer_authority/src/adapters/native-owner-read';
import { readExecutiveMetricProjection } from '../../../packages/server-customer_authority/src/adapters/executive-metric-projections';

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

describe('executive duplicate-review pending metric owner read', () => {
  const nativeSql = "SELECT 'Pending reviews' label,count(*)::text value FROM customer_duplicate_reviews WHERE company_id=? AND status='pending'";
  const actor = { companyId: 'company', holdingSlug: 'acme' };
  const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'USD' };

  it('plans the count on the native owner registry and executes its exact statement', async () => {
    process.env[key] = config();
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { statements: [{ sql: nativeSql, values: ['company'] }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    const seen: { sql: string; values: unknown[] }[] = [];
    const db = {
      prepare: (statement: string) => ({
        bind: (...values: unknown[]) => {
          seen.push({ sql: statement, values });
          return { all: async () => ({ results: [{ label: 'Pending reviews', value: '3' }] }) };
        },
      }),
    };
    const result = await readExecutiveMetricProjection(
      db as never, actor as never, 'duplicate-reviews-pending', filters as never, 'default',
    );
    expect(result.raw).toEqual([{ label: 'Pending reviews', value: '3' }]);
    expect(seen).toEqual([{ sql: nativeSql, values: ['company'] }]);
    const body = JSON.parse(((fetchMock.mock.calls[0] as unknown as [URL, RequestInit])[1] as RequestInit).body as string);
    expect(body.kind).toBe('customer_authority_rd_duplicate_reviews_pending');
    expect(body.values).toEqual(['company']);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('fails closed when the native owner read service is unconfigured', async () => {
    delete process.env[key];
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(readExecutiveMetricProjection(
      { prepare: () => { throw new Error('must not prepare'); } } as never,
      actor as never, 'duplicate-reviews-pending', filters as never, 'default',
    )).rejects.toMatchObject({ code: 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', status: 503 });
    vi.unstubAllGlobals();
  });
});

describe('executive identity-change-events metric owner read', () => {
  const actor = { companyId: 'company', holdingSlug: 'acme' };
  const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'USD' };
  const from = Date.parse(filters.from), to = Date.parse(filters.to) + 86400000;
  const monthSql = "SELECT to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,count(*)::text value FROM customer_identity_change_events e JOIN customer_identity_changes c ON c.id=e.change_id WHERE c.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1";
  const actionSql = "SELECT e.action label,count(*)::text value FROM customer_identity_change_events e JOIN customer_identity_changes c ON c.id=e.change_id WHERE c.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1";

  it('carries the exact identity-change-events breakdown discriminators', () => {
    expect(CA_RD_IDENTITY_CHANGE_EVENTS_MONTH).toBe('customer_authority_rd_identity_change_events_month');
    expect(CA_RD_IDENTITY_CHANGE_EVENTS_ACTION).toBe('customer_authority_rd_identity_change_events_action');
  });

  it.each([
    ['default', CA_RD_IDENTITY_CHANGE_EVENTS_MONTH, monthSql],
    ['month', CA_RD_IDENTITY_CHANGE_EVENTS_MONTH, monthSql],
    ['status', CA_RD_IDENTITY_CHANGE_EVENTS_ACTION, actionSql],
  ] as const)('plans the native %s breakdown and executes its exact statement', async (breakdown, kind, nativeSql) => {
    process.env[key] = config();
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { statements: [{ sql: nativeSql, values: [actor.companyId, from, to] }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    const seen: { sql: string; values: unknown[] }[] = [];
    const db = {
      prepare: (statement: string) => ({
        bind: (...values: unknown[]) => {
          seen.push({ sql: statement, values });
          return { all: async () => ({ results: [{ label: 'x', value: '3' }] }) };
        },
      }),
    };
    const result = await readExecutiveMetricProjection(
      db as never, actor as never, 'identity-change-events', filters as never, breakdown,
    );
    expect(result.raw).toEqual([{ label: 'x', value: '3' }]);
    expect(seen).toEqual([{ sql: nativeSql, values: [actor.companyId, from, to] }]);
    const body = JSON.parse(((fetchMock.mock.calls[0] as unknown as [URL, RequestInit])[1] as RequestInit).body as string);
    expect(body.kind).toBe(kind);
    expect(body.values).toEqual([actor.companyId, from, to]);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('fails closed when the native owner read service is unconfigured', async () => {
    delete process.env[key];
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(readExecutiveMetricProjection(
      { prepare: () => { throw new Error('must not prepare'); } } as never,
      actor as never, 'identity-change-events', filters as never, 'default',
    )).rejects.toMatchObject({ code: 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', status: 503 });
    vi.unstubAllGlobals();
    delete process.env[key];
  });
});

describe('customer authority portal attachment binding-current read family', () => {
  const adapter = (name: string) => new URL(`../../../packages/server-customer_authority/src/adapters/${name}`, import.meta.url);

  it('keeps one fixed-SQL binding/identity literal shared by the read and withdraw fences', async () => {
    const proof = await readFile(adapter('attachment-withdraw-proof.ts'), 'utf8');
    const read = await readFile(adapter('attachment-read.ts'), 'utf8');
    // One canonical declaration; the other surface reuses it by import so the
    // private read and the Files withdraw proof cannot drift into separate SQL.
    expect(proof).toContain('export const bindingIdentityCurrentSql = "SELECT 1 FROM customer_access_bindings binding JOIN customer_access_identities');
    expect(proof.match(/binding\.email=\?"/g)).toHaveLength(1);
    expect(read).not.toContain('binding.email=?');
    expect(read).toContain("import {bindingIdentityCurrentSql} from '@erpx/server-customer_authority/adapters/attachment-withdraw-proof'");
    expect(read).toContain('.prepare(bindingIdentityCurrentSql)');
    expect(proof).toContain('.prepare(bindingIdentityCurrentSql)');
  });
});
