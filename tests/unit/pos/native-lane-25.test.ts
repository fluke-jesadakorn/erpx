import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativePosEvidenceRows,
  nativePosEvidenceConfiguration,
  nativePosEvidenceConfigured,
  type NativePosEvidenceActor,
  type NativePosEvidenceFilters,
} from '../../../modules/pos/server/src/adapters/native-lane-25/client';

const ENV = 'ERPX_NATIVE_POS_LANE25_CONFIG';
const previous = process.env[ENV];
/** Exact development registration: three Holding hosts, one Company each. */
const DEV = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co' },
    { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar', companyId: 'acme-co' },
    { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'acme-co' },
  ],
};
const config = (overrides: Record<string, unknown> = {}) => JSON.stringify({ ...DEV, ...overrides });
const actor = (overrides: Record<string, unknown> = {}): NativePosEvidenceActor => ({
  holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co', branchId: 'br', ...overrides,
} as NativePosEvidenceActor);
const filters = (overrides: Record<string, unknown> = {}): NativePosEvidenceFilters => ({
  metric: 'net-pos-sales', from: '2026-01-01', to: '2026-01-31',
  currency: 'USD', query: 'R1', limit: 25, ...overrides,
} as NativePosEvidenceFilters);
const row = {
  id: 'sale:s1', source_id: 's1', source_type: 'sale', sale_id: 's1',
  event_date: '2026-01-02', receipt_number: 'R1', status: 'completed',
  sale_status: 'completed', currency: 'USD', method: null,
  amount_minor: '100', contribution_minor: '100', refunded_minor: '0',
  revision: '1', sale_version: '1',
};
const ok = (value: unknown) => new Response(JSON.stringify(value), {
  status: 200, headers: { 'content-type': 'application/json' },
});

afterEach(() => {
  vi.unstubAllGlobals();
  if (previous === undefined) delete process.env[ENV];
  else process.env[ENV] = previous;
});

describe('native POS evidence transport (lane 25)', () => {
  it('forwards the actor Holding, Company, scope and exact filters to the owner route', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn().mockResolvedValue(ok({ rows: [row] }));
    vi.stubGlobal('fetch', fetch);
    const result = await nativePosEvidenceRows(actor(), filters({ eventDate: '2026-01-02', id: 'sale:s1' }));
    expect(result).toEqual([row]);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0] as [URL, RequestInit & { body: string; headers: Record<string, string> }];
    expect(url.toString()).toBe('http://127.0.0.1:3198/api/v1/holdings/acme/pos/evidence/search');
    expect(init.method).toBe('POST');
    expect(init.headers['host']).toBe('acme.jesadakorn.com');
    expect(init.headers['origin']).toBe('https://acme.jesadakorn.com');
    expect(init.headers['x-erpx-context-company']).toBe('acme-co');
    expect(init.headers['x-erpx-workspace-branch']).toBe('br');
    expect(init.headers['content-type']).toBe('application/json');
    expect(JSON.parse(init.body)).toEqual({
      metric: 'net-pos-sales', from: '2026-01-01', to: '2026-01-31',
      currency: 'USD', query: 'R1', limit: 25,
      cursor: { eventDate: '2026-01-02', id: 'sale:s1' },
    });
  });

  it('routes each of the three registered Holdings to its own host and path', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn().mockImplementation(async () => ok({ rows: [row] }));
    vi.stubGlobal('fetch', fetch);
    const cases = [
      ['h001', 'acme', 'acme.jesadakorn.com'],
      ['h002', 'northstar', 'northstar.jesadakorn.com'],
      ['h003', 'erpx', 'erpx-internal.jesadakorn.com'],
    ];
    for (const [index, [holdingId, holdingSlug, host]] of cases.entries()) {
      await expect(nativePosEvidenceRows(actor({ holdingId, holdingSlug }), filters())).resolves.toEqual([row]);
      const [url, init] = fetch.mock.calls[index] as [URL, RequestInit & { headers: Record<string, string> }];
      expect(url.toString()).toBe(`http://127.0.0.1:3198/api/v1/holdings/${holdingSlug}/pos/evidence/search`);
      expect(init.headers['host']).toBe(host);
      expect(init.headers['origin']).toBe(`https://${host}`);
      expect(init.headers['x-erpx-context-company']).toBe('acme-co');
    }
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it('rejects unknown WorkContext targets and never falls back to the first Holding', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    for (const bad of [
      actor({ holdingId: 'h404' }),
      actor({ holdingSlug: 'unknown' }),
      actor({ holdingId: 'h002', holdingSlug: 'acme' }),
    ])
      await expect(nativePosEvidenceRows(bad, filters())).rejects.toMatchObject({
        code: 'WORK_CONTEXT_CHANGED', status: 409,
      });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('accepts the pinned 15-key row and rejects malformed rows', async () => {
    process.env[ENV] = config();
    const client = (value: unknown) => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ rows: [value] })));
      return nativePosEvidenceRows(actor(), filters());
    };
    await expect(client(row)).resolves.toEqual([row]);
    for (const bad of [
      { ...row, extra: true }, { ...row, id: undefined }, { ...row, source_type: 'other' },
      { ...row, id: 'refund:s1' }, { ...row, amount_minor: 100 },
      { ...row, event_date: 'not-a-date' }, { ...row, method: 7 },
    ])
      await expect(client(bad)).rejects.toMatchObject({
        code: 'NATIVE_POS_LANE25_RESPONSE_INVALID', status: 503,
      });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ rows: 'nope' })));
    await expect(nativePosEvidenceRows(actor(), filters())).rejects.toMatchObject({
      code: 'NATIVE_POS_LANE25_RESPONSE_INVALID', status: 503,
    });
  });

  it('fails closed when unconfigured, unreachable or the service errors', async () => {
    delete process.env[ENV];
    await expect(nativePosEvidenceRows(actor(), filters())).rejects.toMatchObject({
      code: 'NATIVE_POS_LANE25_CONFIGURATION_REQUIRED', status: 503,
    });
    process.env[ENV] = config();
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')));
    await expect(nativePosEvidenceRows(actor(), filters())).rejects.toMatchObject({
      code: 'NATIVE_POS_LANE25_UNAVAILABLE', status: 503,
    });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(
      JSON.stringify({ error: { code: 'POS_EVIDENCE_QUERY_INVALID', message: 'Bad filters.' } }),
      { status: 422, headers: { 'content-type': 'application/json' } },
    )));
    await expect(nativePosEvidenceRows(actor(), filters())).rejects.toMatchObject({
      code: 'POS_EVIDENCE_QUERY_INVALID', status: 422, message: 'Bad filters.',
    });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status: 500 })));
    await expect(nativePosEvidenceRows(actor(), filters())).rejects.toMatchObject({
      code: 'NATIVE_POS_LANE25_UNAVAILABLE', status: 503,
    });
  });

  it('rejects out-of-scope actors and filters before any fetch', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    for (const bad of [
      actor({ companyId: '' }), actor({ branchId: 'bad branch' }),
    ])
      await expect(nativePosEvidenceRows(bad, filters())).rejects.toMatchObject({
        code: 'NATIVE_POS_LANE25_REQUEST_INVALID', status: 422,
      });
    for (const bad of [
      { ...filters(), metric: 'other' }, { ...filters(), limit: 51 },
      { ...filters(), currency: 'usd' }, { ...filters(), query: 'x'.repeat(101) },
    ])
      await expect(nativePosEvidenceRows(actor(), bad as never)).rejects.toMatchObject({
        code: 'NATIVE_POS_LANE25_REQUEST_INVALID', status: 422,
      });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps 1..64 holding registration strict', () => {
    expect(nativePosEvidenceConfigured({ [ENV]: config() })).toBe(true);
    expect(nativePosEvidenceConfigured({})).toBe(false);
    expect(nativePosEvidenceConfiguration(config()).holdings).toHaveLength(3);
    expect(nativePosEvidenceConfiguration(config()).holdings[2].companyId).toBe('acme-co');
    expect(() => nativePosEvidenceConfiguration(config({ holdings: [] }))).toThrow();
    expect(() => nativePosEvidenceConfiguration(config({
      holdings: Array.from({ length: 65 }, (_, i) => ({
        origin: `https://h${i}.example.com`, host: `h${i}.example.com`,
        holdingId: `x${i}`, holdingSlug: `h${i}`,
      })),
    }))).toThrow();
    expect(() => nativePosEvidenceConfiguration(config({
      holdings: [DEV.holdings[0], { ...DEV.holdings[0] }],
    }))).toThrow();
    expect(() => nativePosEvidenceConfiguration(config({
      holdings: [{ ...DEV.holdings[0], host: 'wrong.example.com' }],
    }))).toThrow();
    expect(() => nativePosEvidenceConfiguration(config({
      holdings: [{ ...DEV.holdings[0], companyId: 'bad company' }],
    }))).toThrow();
    expect(() => nativePosEvidenceConfiguration(config({
      privateBaseUrl: 'http://127.0.0.1:3100',
    }))).toThrow();
    expect(() => nativePosEvidenceConfiguration(config({
      holdings: [{ origin: 'http://insecure.example.com', host: 'insecure.example.com', holdingId: 'h001', holdingSlug: 'acme' }],
    }))).toThrow();
  });

  it('leaves no SELECT literal in the retired TypeScript source', () => {
    const source = readFileSync(
      new URL('../../../modules/pos/server/src/adapters/executive-evidence-sources.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toMatch(/\bSELECT\b/);
    expect(source).toContain('nativePosEvidenceRows');
  });
});
