import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativePosAll } from '../../../packages/server-pos/src/adapters/native-owner-write';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;

afterEach(() => {
  vi.unstubAllGlobals();
  if (previous === undefined) delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
  else process.env.ERPX_NATIVE_POS_OWNER_CONFIG = previous;
});

function mockDb(seen: { sql?: string; values?: unknown[] }, rows: unknown[] = []) {
  return {
    prepare: (sql: string) => ({
      bind: (...values: unknown[]) => {
        seen.sql = sql;
        seen.values = values;
        return { all: async () => ({ results: rows, success: true, meta: { changes: 0, duration: 0 } }) };
      },
    }),
  };
}

describe('native POS approval-queue read facade', () => {
  it('fetches the owner statement and binds values on the caller database', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const fetch = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({ data: { sql: "SELECT * FROM pos_jobs WHERE company_id=? AND state='exception'", values: ['company'] } }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const rows = [{ id: 'job-exc' }];
    const result = await nativePosAll(mockDb(seen, rows) as never, 'company', 'pos_readposjobs1_245', ['company']);
    expect(result).toEqual(rows);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('company');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind: 'pos_readposjobs1_245', values: ['company'] });
    expect(seen.sql).toBe("SELECT * FROM pos_jobs WHERE company_id=? AND state='exception'");
    expect(seen.values).toEqual(['company']);
  });

  it('binds branch scope and independence filters positionally for queue reads', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const statement = "SELECT * FROM pos_refunds WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND state='requested' AND requested_by<>?";
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ data: { sql: statement, values: ['company', 'branch', 'branch', 'bob'] } }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );
    const seen: { sql?: string; values?: unknown[] } = {};
    await nativePosAll(mockDb(seen) as never, 'company', 'pos_readposrefunds3_230', ['company', 'branch', 'branch', 'bob']);
    expect(seen.sql).toBe(statement);
    expect(seen.values).toEqual(['company', 'branch', 'branch', 'bob']);
  });

  it('fails closed when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativePosAll(mockDb({}) as never, 'company', 'pos_readposjobs1_245', ['company'])).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('surfaces the bounded owner error and rejects malformed statements', async () => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ error: { code: 'POS_OWNER_WRITE_INVALID', message: 'Review.' } }), {
          status: 422,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );
    await expect(nativePosAll(mockDb({}) as never, 'company', 'nope', [])).rejects.toMatchObject({
      code: 'POS_OWNER_WRITE_INVALID',
      status: 422,
    });
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(JSON.stringify({ data: { sql: 42, values: [] } }), { status: 200 })),
    );
    await expect(nativePosAll(mockDb({}) as never, 'company', 'pos_readposjobs1_245', ['company'])).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_RESPONSE_INVALID',
      status: 503,
    });
  });
});
