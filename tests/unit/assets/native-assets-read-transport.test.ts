import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativeAssetsPlan,
  nativeAssetsReadAll,
  ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE,
  ASSETS_RD_OBLIGATIONS_BASE,
  ASSETS_RD_PKG_LINKS,
} from '../../../packages/server-assets/src/adapters/native-assets-read';

const CONFIG_KEY = 'ERPX_NATIVE_ASSETS_READ_CONFIG';
const FALLBACK_KEY = 'ERPX_NATIVE_ASSETS_CONFIG';
const originals = new Map([CONFIG_KEY, FALLBACK_KEY].map((key) => [key, process.env[key]]));
const ok = (value: unknown) => new Response(JSON.stringify(value), {
  status: 200,
  headers: { 'content-type': 'application/json' },
});

const statement = { sql: 'SELECT to_jsonb(source)::text image FROM operational_assets source WHERE source.id=? AND source.company_id=?', values: ['asset-1', 'company-1'] };
const cteStatement = {
  sql: 'WITH scope AS(SELECT ?::text company_id,?::boolean is_base) SELECT * FROM scope',
  values: ['company-1', 'true'],
};

describe('native Assets read-plan transport', () => {
  beforeEach(() => {
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44995',
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
      return ok({ data: { kind: ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE, caller: 'field_service.attachment-source', statement } });
    }));
    const planned = await nativeAssetsPlan(ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE, {
      assetId: 'asset-1', companyId: 'company-1',
    });
    expect(planned.sql).toBe(statement.sql);
    expect(planned.values).toEqual(['asset-1', 'company-1']);
    expect(url).toBe('http://127.0.0.1:44995/api/v1/holdings/acme/assets/read-plan');
    expect(init?.method).toBe('POST');
    expect((init?.headers as Headers).get('host')).toBe('acme.example.com');
    expect((init?.headers as Headers).get('origin')).toBe('https://acme.example.com');
    expect(JSON.parse(String(init?.body))).toEqual({
      kind: ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE,
      scope: { assetId: 'asset-1', companyId: 'company-1' },
    });
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
      data: { kind: ASSETS_RD_OBLIGATIONS_BASE, caller: 'assets.package-obligations', statement: cteStatement },
    })));
    const result = await nativeAssetsReadAll<{ id: string }>(ASSETS_RD_OBLIGATIONS_BASE, {
      companyId: 'company-1', isBase: true,
    }, db);
    expect(result.results).toEqual([{ id: 'asset-1' }]);
    expect(prepares).toEqual([cteStatement.sql]);
    expect(binds).toEqual([['company-1', 'true']]);
  });

  it('accepts a CTE-led owner statement', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ok({
      data: { kind: ASSETS_RD_OBLIGATIONS_BASE, caller: 'assets.package-obligations', statement: cteStatement },
    })));
    const planned = await nativeAssetsPlan(ASSETS_RD_OBLIGATIONS_BASE, { companyId: 'company-1', isBase: true });
    expect(planned.sql).toBe(cteStatement.sql);
    expect(planned.values).toEqual(['company-1', 'true']);
  });

  it('fails closed for unknown kinds, absent config and rejected or malformed responses', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(nativeAssetsPlan('bogus.read')).rejects.toMatchObject({
      code: 'ASSETS_READ_PLAN_INVALID',
    });
    await expect(nativeAssetsPlan(ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE, { assetId: 'x'.repeat(201) }))
      .rejects.toMatchObject({ code: 'ASSETS_READ_PLAN_INVALID' });
    delete process.env[CONFIG_KEY];
    delete process.env[FALLBACK_KEY];
    await expect(nativeAssetsPlan(ASSETS_RD_PKG_LINKS))
      .rejects.toMatchObject({ code: 'ASSETS_NATIVE_READ_UNAVAILABLE', status: 503 });
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44995',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme' }],
    });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      error: { code: 'ORIGIN_FORBIDDEN', message: 'Use the registered application origin.' },
    }), { status: 403 })));
    await expect(nativeAssetsPlan(ASSETS_RD_PKG_LINKS))
      .rejects.toMatchObject({ code: 'ORIGIN_FORBIDDEN', status: 403 });
    vi.stubGlobal('fetch', vi.fn(async () => ok({
      data: { kind: ASSETS_RD_PKG_LINKS, caller: 'assets.package-obligations',
        statement: { sql: 'DROP TABLE operational_assets', values: [] } },
    })));
    await expect(nativeAssetsPlan(ASSETS_RD_PKG_LINKS))
      .rejects.toMatchObject({ code: 'ASSETS_NATIVE_READ_UNAVAILABLE' });
  });

  it('rejects malformed configuration before any fetch', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3100', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme' }],
    });
    await expect(nativeAssetsPlan(ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE))
      .rejects.toMatchObject({ code: 'ASSETS_NATIVE_READ_CONFIG_INVALID' });
    process.env[CONFIG_KEY] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44995', timeoutMs: 1000,
      holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme' }],
    });
    await expect(nativeAssetsPlan(ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE))
      .rejects.toMatchObject({ code: 'ASSETS_NATIVE_READ_CONFIG_INVALID' });
  });
});
