import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import { posAttachmentReadSourceImage } from '../../../modules/pos/server/src/adapters/attachment-read-sources';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;

afterEach(() => {
  vi.unstubAllGlobals();
  if (previous === undefined) delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
  else process.env.ERPX_NATIVE_POS_OWNER_CONFIG = previous;
});

function mockDb(seen: { sql?: string; values?: unknown[] }, image: string | null = null) {
  return {
    prepare: (sql: string) => ({
      bind: (...values: unknown[]) => {
        seen.sql = sql;
        seen.values = values;
        return { first: async (column?: string) => (column === 'image' ? image : null) };
      },
    }),
  };
}

function stubFetch(sql: string, image: string | null) {
  const fetch = vi.fn().mockResolvedValue(
    new Response(JSON.stringify({ data: { sql, values: ['source', 'company'] } }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    }),
  );
  vi.stubGlobal('fetch', fetch);
  return fetch;
}

describe('native POS attachment source read bridge', () => {
  it.each([
    ['pos_sale', 'pos_attachment_source_pos_sale', 'SELECT to_jsonb(source)::text image FROM pos_sales source'],
    ['pos_refund', 'pos_attachment_source_pos_refund', 'SELECT to_jsonb(source)::text image FROM pos_refunds source'],
    ['pos_shift', 'pos_attachment_source_pos_shift', 'SELECT to_jsonb(source)::text image FROM pos_shifts source'],
  ] as const)('requests the owner kind for %s with id/Company scope only', async (target, kind, sql) => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const fetch = stubFetch(sql, '{"id":"source"}');
    const seen: { sql?: string; values?: unknown[] } = {};
    const result = await posAttachmentReadSourceImage(mockDb(seen, '{"id":"source"}') as never, 'company', target, 'source');
    expect(result).toBe('{"id":"source"}');
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies/company/pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('company');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: 'company', kind, values: ['source', 'company'] });
    expect(seen.sql).toBe(sql);
    expect(seen.values).toEqual(['source', 'company']);
  });

  it('fails closed when the native owner service is unconfigured', async () => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(posAttachmentReadSourceImage(mockDb({}) as never, 'company', 'pos_sale', 'source')).rejects.toMatchObject({
      code: 'NATIVE_POS_OWNER_UNAVAILABLE',
      status: 503,
    });
    expect(fetch).not.toHaveBeenCalled();
  });
});
