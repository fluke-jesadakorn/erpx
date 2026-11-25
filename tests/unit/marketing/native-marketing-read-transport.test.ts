import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativeMarketingPlan,
  nativeMarketingReadAll,
  MARKETING_RD_ASSET_POINT,
  MARKETING_RD_FORM_CONTENT_ROW,
  MARKETING_RD_CAMPAIGN_EXISTS,
  MARKETING_RD_PACKAGE_OBLIGATIONS,
  MARKETING_RD_COMMAND_RECEIPT_PRIOR,
} from '../../../packages/server-marketing/src/adapters/native-marketing-read';

const CONFIG_KEY = 'ERPX_NATIVE_MARKETING_READ_CONFIG';
const FALLBACK_KEY = 'ERPX_NATIVE_MARKETING_CONFIG';
const originals = new Map([CONFIG_KEY, FALLBACK_KEY].map((key) => [key, process.env[key]]));
const ok = (value: unknown) => new Response(JSON.stringify(value), {
  status: 200,
  headers: { 'content-type': 'application/json' },
});

const statement = { sql: 'SELECT * FROM marketing_assets WHERE id=? AND company_id=?', values: ['asset-1', 'company-1'] };

describe('native Marketing read-plan transport', () => {
  beforeEach(() => {
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44994',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
    });
  });
  afterEach(() => {
    for (const [key, value] of originals) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    vi.unstubAllGlobals();
  });

  it('posts the closed kind and bounded scope to the exact Holding route', async () => {
    let url = '';
    let init: RequestInit | undefined;
    vi.stubGlobal('fetch', vi.fn(async (u: URL, i: RequestInit) => {
      url = String(u);
      init = i;
      return ok({ data: { kind: MARKETING_RD_ASSET_POINT, caller: 'marketing.workspace.asset', statement } });
    }));
    const planned = await nativeMarketingPlan(MARKETING_RD_ASSET_POINT, {
      assetId: 'asset-1', companyId: 'company-1',
    });
    expect(planned.sql).toBe(statement.sql);
    expect(planned.values).toEqual(['asset-1', 'company-1']);
    expect(url).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/marketing/read-plan');
    expect(init?.method).toBe('POST');
    expect((init?.headers as Headers).get('host')).toBe('acme.example.com');
    expect((init?.headers as Headers).get('origin')).toBe('https://acme.example.com');
    expect(JSON.parse(String(init?.body))).toEqual({
      kind: MARKETING_RD_ASSET_POINT,
      scope: { assetId: 'asset-1', companyId: 'company-1' },
    });
  });

  it('accepts the registered package-obligations kind and binds only the company', async () => {
    let body: unknown;
    vi.stubGlobal('fetch', vi.fn(async (u: URL, i: RequestInit) => {
      body = JSON.parse(String(i.body));
      return ok({ data: { kind: MARKETING_RD_PACKAGE_OBLIGATIONS, caller: 'marketing.package-obligations', statement } });
    }));
    const planned = await nativeMarketingPlan(MARKETING_RD_PACKAGE_OBLIGATIONS, { companyId: 'company-1' });
    expect(planned.kind).toBe('marketing-package-obligations');
    expect(body).toEqual({
      kind: MARKETING_RD_PACKAGE_OBLIGATIONS,
      scope: { companyId: 'company-1' },
    });
  });

  it('plans the command-receipt prior guard with five ordered scope values', async () => {
    let body: unknown;
    const sql = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
    vi.stubGlobal('fetch', vi.fn(async (u: URL, i: RequestInit) => {
      body = JSON.parse(String(i.body));
      return ok({ data: { kind: MARKETING_RD_COMMAND_RECEIPT_PRIOR, caller: 'marketing.purchase.prior-receipt', statement: { sql, values: ['command-1', 'marketing.asset.activate', 'record-1', 'actor-1', 'company-1'] } } });
    }));
    const planned = await nativeMarketingPlan(MARKETING_RD_COMMAND_RECEIPT_PRIOR, {
      commandId: 'command-1', operation: 'marketing.asset.activate', recordId: 'record-1',
      actorId: 'actor-1', companyId: 'company-1',
    });
    expect(body).toEqual({
      kind: MARKETING_RD_COMMAND_RECEIPT_PRIOR,
      scope: { commandId: 'command-1', operation: 'marketing.asset.activate', recordId: 'record-1', actorId: 'actor-1', companyId: 'company-1' },
    });
    expect(planned.sql).toBe(sql);
    expect(planned.values).toEqual(['command-1', 'marketing.asset.activate', 'record-1', 'actor-1', 'company-1']);
  });

  it('executes the owner statement on the caller unit, not on the transport', async () => {
    const prepares: string[] = [];
    const binds: unknown[][] = [];
    const db = {
      prepare(sql: string) {
        prepares.push(sql);
        return {
          bind(...values: unknown[]) {
            binds.push(values);
            return { sql, values };
          },
        };
      },
      batch: async () => [{ results: [{ id: 'asset-1' }], success: true }],
    } as never;
    vi.stubGlobal('fetch', vi.fn(async () => ok({
      data: { kind: MARKETING_RD_ASSET_POINT, caller: 'marketing.workspace.asset', statement },
    })));
    const result = await nativeMarketingReadAll<{ id: string }>(MARKETING_RD_ASSET_POINT, {
      assetId: 'asset-1', companyId: 'company-1',
    }, db);
    expect(result.results).toEqual([{ id: 'asset-1' }]);
    expect(prepares).toEqual([statement.sql]);
    expect(binds).toEqual([['asset-1', 'company-1']]);
  });

  it('fails closed for unknown kinds, absent config and rejected or malformed responses', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(nativeMarketingPlan('bogus.read')).rejects.toMatchObject({
      code: 'MARKETING_READ_PLAN_INVALID',
    });
    await expect(nativeMarketingPlan(MARKETING_RD_ASSET_POINT, { assetId: 'x'.repeat(201) }))
      .rejects.toMatchObject({ code: 'MARKETING_READ_PLAN_INVALID' });
    delete process.env[CONFIG_KEY];
    delete process.env[FALLBACK_KEY];
    await expect(nativeMarketingPlan(MARKETING_RD_FORM_CONTENT_ROW))
      .rejects.toMatchObject({ code: 'MARKETING_NATIVE_READ_UNAVAILABLE', status: 503 });
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44994',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme' }],
    });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      error: { code: 'ORIGIN_FORBIDDEN', message: 'Use the registered application origin.' },
    }), { status: 403 })));
    await expect(nativeMarketingPlan(MARKETING_RD_CAMPAIGN_EXISTS))
      .rejects.toMatchObject({ code: 'ORIGIN_FORBIDDEN', status: 403 });
    vi.stubGlobal('fetch', vi.fn(async () => ok({
      data: { kind: MARKETING_RD_CAMPAIGN_EXISTS, caller: 'campaigns.assertion',
        statement: { sql: 'DROP TABLE marketing_assets', values: [] } },
    })));
    await expect(nativeMarketingPlan(MARKETING_RD_CAMPAIGN_EXISTS))
      .rejects.toMatchObject({ code: 'MARKETING_NATIVE_READ_UNAVAILABLE' });
  });

  it('rejects malformed configuration before any fetch', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3100', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme' }],
    });
    await expect(nativeMarketingPlan(MARKETING_RD_ASSET_POINT))
      .rejects.toMatchObject({ code: 'MARKETING_NATIVE_READ_CONFIG_INVALID' });
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000,
      holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme' }],
    });
    await expect(nativeMarketingPlan(MARKETING_RD_ASSET_POINT))
      .rejects.toMatchObject({ code: 'MARKETING_NATIVE_READ_CONFIG_INVALID' });
  });
});
