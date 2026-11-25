import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';

vi.mock('server-only', () => ({}));

import {
  NATIVE_FINANCE_LANE_2_CONFIG_KEY,
  financeOverviewCountsNative,
  nativeLane2Config,
  nativeLane2Configured,
} from '../../../packages/server-finance/src/adapters/native-lane-2/client';

const actor = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme:co-1' };
const counts = {
  journals: 12,
  posting: 3,
  invoices: 40,
  payments: 9,
  receivable: 7,
  payable: 2,
  reconcile: 1,
};
const original = process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY];

const setConfig = () => {
  process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44996',
    timeoutMs: 1000,
    holdings: [
      {
        origin: 'https://acme.example.com',
        host: 'acme.example.com',
        holdingId: 'h001',
        holdingSlug: 'acme',
        companyId: 'acme:co-1',
      },
      {
        origin: 'https://northstar.example.com',
        host: 'northstar.example.com',
        holdingId: 'h002',
        holdingSlug: 'northstar',
        companyId: 'northstar:co-1',
      },
      {
        origin: 'https://erpx.example.com',
        host: 'erpx.example.com',
        holdingId: 'h003',
        holdingSlug: 'erpx',
        companyId: 'erpx:co-1',
      },
    ],
  });
};

const jsonResponse = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });

beforeEach(() => {
  setConfig();
});
afterEach(() => {
  if (original === undefined) delete process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY];
  else process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY] = original;
  vi.unstubAllGlobals();
});

describe('native Finance lane 2 overview counts', () => {
  it('carries no SQL in the client and no replaced SQL in the adapter', () => {
    const client = readFileSync(
      new URL('../../../packages/server-finance/src/adapters/native-lane-2/client.ts', import.meta.url),
      'utf8',
    );
    expect(client).not.toMatch(/\bSELECT\b|\bINSERT\b|\bUPDATE\b|\bDELETE\b/);
    const adapter = readFileSync(
      new URL('../../../packages/server-finance/src/adapters/finance-invoice-read.ts', import.meta.url),
      'utf8',
    );
    expect(adapter).not.toContain('companyCountSql');
    expect(adapter).not.toContain('UNMATCHED_BANK_TXN_COUNT_SQL');
    expect(adapter).toContain('financeOverviewCountsNative(actor)');
  });

  it('pins the response shape and the owner-local transport', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => jsonResponse({ data: counts })),
    );
    expect(await financeOverviewCountsNative(actor)).toEqual(counts);

    const [url, init] = vi.mocked(fetch).mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44996/api/v1/holdings/acme/finance/native-lane-2/overview-counts',
    );
    const headers = init.headers as Record<string, string>;
    expect(headers.host).toBe('acme.example.com');
    expect(headers['x-erpx-context-company']).toBe('acme:co-1');
    expect(headers.origin).toBe('https://acme.example.com');
    expect(init.cache).toBe('no-store');
  });

  it('accepts the exact dev env JSON for all three registered Holdings', () => {
    const devEnv = {
      [NATIVE_FINANCE_LANE_2_CONFIG_KEY]: JSON.stringify({
        privateBaseUrl: 'http://127.0.0.1:3198',
        timeoutMs: 10000,
        holdings: [
          { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
          { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
          { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
        ],
      }),
    };
    const config = nativeLane2Config(devEnv);
    expect(config.base).toBe('http://127.0.0.1:3198');
    expect(config.timeoutMs).toBe(10000);
    expect(config.holdings).toEqual([
      { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
      { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
      { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
    ]);
  });

  it('selects every registered Holding by WorkContext with no first-holding default', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ data: counts })));
    const holdings = [
      { holdingId: 'h002', holdingSlug: 'northstar', companyId: 'northstar:co-1', host: 'northstar.example.com' },
      { holdingId: 'h003', holdingSlug: 'erpx', companyId: 'erpx:co-1', host: 'erpx.example.com' },
      { holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme:co-1', host: 'acme.example.com' },
    ];
    for (const [i, holding] of holdings.entries()) {
      await expect(financeOverviewCountsNative(holding)).resolves.toEqual(counts);
      const [url, init] = vi.mocked(fetch).mock.calls[i] as unknown as [URL, RequestInit];
      expect(url.pathname).toBe(
        `/api/v1/holdings/${holding.holdingSlug}/finance/native-lane-2/overview-counts`,
      );
      expect((init.headers as Record<string, string>).host).toBe(holding.host);
      expect((init.headers as Record<string, string>)['x-erpx-context-company']).toBe(
        holding.companyId,
      );
    }
    await expect(
      financeOverviewCountsNative({ ...actor, holdingId: 'h999' }),
    ).rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    await expect(
      financeOverviewCountsNative({ ...actor, companyId: 'other:co' }),
    ).rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
  });

  it('fails closed on missing configuration', async () => {
    delete process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY];
    expect(nativeLane2Configured()).toBe(false);
    await expect(financeOverviewCountsNative(actor)).rejects.toMatchObject({
      code: 'FINANCE_NATIVE_LANE_2_UNAVAILABLE',
      status: 503,
    });
  });

  it('fails closed on a changed Holding context', async () => {
    await expect(
      financeOverviewCountsNative({ ...actor, holdingSlug: 'other' }),
    ).rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
  });

  it('fails closed on an invalid Company identifier', async () => {
    await expect(
      financeOverviewCountsNative({ ...actor, companyId: 'bad/id' }),
    ).rejects.toMatchObject({ code: 'FINANCE_NATIVE_LANE_2_UNAVAILABLE', status: 422 });
  });

  it('fails closed on malformed, partial or non-200 replies', async () => {
    for (const body of [
      { data: { ...counts, reconcile: -1 } },
      { data: { ...counts, posting: '3' } },
      { data: { journals: counts.journals } },
      { wrong: counts },
    ]) {
      vi.stubGlobal('fetch', vi.fn(async () => jsonResponse(body)));
      await expect(financeOverviewCountsNative(actor)).rejects.toMatchObject({
        code: 'FINANCE_NATIVE_LANE_2_UNAVAILABLE',
        status: 502,
      });
    }
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ data: counts }, 503)));
    await expect(financeOverviewCountsNative(actor)).rejects.toMatchObject({ status: 503 });

    vi.stubGlobal('fetch', vi.fn(async () => new Response('{}', { status: 200 })));
    await expect(financeOverviewCountsNative(actor)).rejects.toMatchObject({ status: 502 });
  });

  it('fails closed when the native service cannot be reached', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => Promise.reject(new Error('offline'))));
    await expect(financeOverviewCountsNative(actor)).rejects.toMatchObject({
      code: 'FINANCE_NATIVE_LANE_2_UNAVAILABLE',
      status: 503,
    });
  });
});
