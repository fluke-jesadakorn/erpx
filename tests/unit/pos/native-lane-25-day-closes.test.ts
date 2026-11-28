import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import { nativePosDayCloses } from '../../../modules/pos/server/src/adapters/native-lane-25/day-closes-client';
import { nativePosEvidenceConfiguration } from '../../../modules/pos/server/src/adapters/native-lane-25/client';

const ENV = 'ERPX_NATIVE_POS_LANE25_CONFIG';
const previous = process.env[ENV];
const DEV = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co' },
    { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar', companyId: 'acme-co' },
  ],
};
const config = (overrides: Record<string, unknown> = {}) => JSON.stringify({ ...DEV, ...overrides });
const actor = (overrides: Record<string, unknown> = {}) => ({
  holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co', branchId: 'br', ...overrides,
});
const validRow = {
  id: 'dc1', company_id: 'acme-co', branch_id: 'br', business_date: '2026-01-31',
  currency: 'USD', state: 'confirmed', prepared_by: 'u1', confirmed_by: 'u2',
  version: '1', snapshot_json: '{}',
};
const ok = (value: unknown) => new Response(JSON.stringify(value), {
  status: 200, headers: { 'content-type': 'application/json' },
});

afterEach(() => {
  vi.unstubAllGlobals();
  if (previous === undefined) delete process.env[ENV];
  else process.env[ENV] = previous;
});

describe('native POS day-close report transport (lane 25 wave 3)', () => {
  it('forwards the actor Holding, Company and branch to the owner route', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn().mockResolvedValue(ok({ rows: [validRow] }));
    vi.stubGlobal('fetch', fetch);
    const result = await nativePosDayCloses(actor());
    expect(result).toEqual([validRow]);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0] as [URL, RequestInit & { headers: Record<string, string> }];
    expect(url.toString()).toBe('http://127.0.0.1:3198/api/v1/holdings/acme/pos/day-closes');
    expect(init.method).toBe('POST');
    expect(init.headers['host']).toBe('acme.jesadakorn.com');
    expect(init.headers['origin']).toBe('https://acme.jesadakorn.com');
    expect(init.headers['x-erpx-context-company']).toBe('acme-co');
    expect(init.headers['x-erpx-workspace-branch']).toBe('br');
  });

  it('rejects unknown WorkContext targets and never falls back to the first Holding', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    for (const bad of [actor({ holdingId: 'h404' }), actor({ holdingSlug: 'unknown' }), actor({ holdingId: 'h002', holdingSlug: 'acme' })])
      await expect(nativePosDayCloses(bad)).rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('accepts the pinned 10-key row and rejects malformed rows', async () => {
    process.env[ENV] = config();
    const client = (value: unknown) => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ rows: [value] })));
      return nativePosDayCloses(actor());
    };
    await expect(client(validRow)).resolves.toEqual([validRow]);
    for (const bad of [
      { ...validRow, extra: true }, { ...validRow, id: undefined },
      { ...validRow, business_date: 'not-a-date' }, { ...validRow, snapshot_json: 7 },
    ])
      await expect(client(bad)).rejects.toMatchObject({ code: 'NATIVE_POS_LANE25_DAY_CLOSES_RESPONSE_INVALID', status: 503 });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ rows: 'nope' })));
    await expect(nativePosDayCloses(actor())).rejects.toMatchObject({
      code: 'NATIVE_POS_LANE25_DAY_CLOSES_RESPONSE_INVALID', status: 503,
    });
  });

  it('fails closed when unconfigured, unreachable or the service errors', async () => {
    delete process.env[ENV];
    await expect(nativePosDayCloses(actor())).rejects.toMatchObject({
      code: 'NATIVE_POS_LANE25_CONFIGURATION_REQUIRED', status: 503,
    });
    process.env[ENV] = config();
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')));
    await expect(nativePosDayCloses(actor())).rejects.toMatchObject({
      code: 'NATIVE_POS_LANE25_DAY_CLOSES_UNAVAILABLE', status: 503,
    });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(
      JSON.stringify({ error: { code: 'POS_EVIDENCE_QUERY_INVALID', message: 'Bad scope.' } }),
      { status: 422, headers: { 'content-type': 'application/json' } },
    )));
    await expect(nativePosDayCloses(actor())).rejects.toMatchObject({
      code: 'POS_EVIDENCE_QUERY_INVALID', status: 422, message: 'Bad scope.',
    });
  });

  it('rejects out-of-scope actors before any fetch', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    for (const bad of [actor({ companyId: '' }), actor({ branchId: 'bad branch' })])
      await expect(nativePosDayCloses(bad)).rejects.toMatchObject({
        code: 'NATIVE_POS_LANE25_DAY_CLOSES_REQUEST_INVALID', status: 422,
      });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps the Holding registration strict', () => {
    expect(nativePosEvidenceConfiguration(config()).holdings).toHaveLength(2);
    expect(nativePosEvidenceConfiguration(config()).holdings[1].companyId).toBe('acme-co');
  });

  it('leaves no SELECT literal in the retired TypeScript source', () => {
    const source = readFileSync(
      new URL('../../../modules/pos/server/src/adapters/pos.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('FROM pos_day_closes WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY business_date DESC LIMIT 90');
    expect(source).toContain('nativePosDayCloses');
  });
});
