import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativeCommercialOwnerRead,
  COMMERCIAL_RD_USAGE_METER,
  COMMERCIAL_RD_USAGE_SNAPSHOT,
  COMMERCIAL_RD_USAGE_SEAT,
} from '../../../packages/server-commercial/src/adapters/native-owner-read';

describe('native Commercial owner-read client', () => {
  const envKeys = [
    'ERPX_NATIVE_COMMERCIAL_OWNER_READ_CONFIG',
    'ERPX_NATIVE_COMMERCIAL_CONFIG',
    'ERPX_NATIVE_BILLING_CONFIG',
    'ERPX_NATIVE_COMMERCIAL_OWNER_WRITE_CONFIG',
  ] as const;
  const originals = new Map(envKeys.map((key) => [key, process.env[key]]));
  const envKey = envKeys[0];
  const snapshotSql = 'SELECT "holding_id","month" FROM billing_usage_snapshots';
  beforeEach(() => {
    process.env[envKey] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44991',
      host: 'acme.example.test',
      holdingSlug: 'acme',
      origin: 'https://acme.example.test',
      timeoutMs: 1000,
    });
  });
  afterEach(() => {
    for (const [key, value] of originals) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    vi.unstubAllGlobals();
  });

  it('returns the owner-planned SELECT for each closed usage kind', async () => {
    const kinds: string[] = [];
    const paths: string[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: URL, init: RequestInit) => {
      paths.push(String(url));
      kinds.push((JSON.parse(String(init.body)) as { kind: string }).kind);
      return new Response(JSON.stringify({ data: { sql: snapshotSql } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }));
    for (const kind of [COMMERCIAL_RD_USAGE_SNAPSHOT, COMMERCIAL_RD_USAGE_METER, COMMERCIAL_RD_USAGE_SEAT]) {
      expect(await nativeCommercialOwnerRead(kind)).toBe(snapshotSql);
    }
    expect(kinds).toEqual(['usage.snapshot', 'usage.meter', 'usage.seat']);
    expect(paths.every((path) => path.includes('/commercial/owner-read'))).toBe(true);
  });

  it('fails closed when configuration is missing', async () => {
    delete process.env[envKey];
    for (const key of ['ERPX_NATIVE_COMMERCIAL_CONFIG', 'ERPX_NATIVE_BILLING_CONFIG', 'ERPX_NATIVE_COMMERCIAL_OWNER_WRITE_CONFIG']) delete process.env[key];
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_METER))
      .rejects.toMatchObject({ code: 'COMMERCIAL_OWNER_READ_UNAVAILABLE', status: 503 });
  });

  it('fails closed on a non-SELECT or unavailable owner response', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('down', { status: 503 })));
    await expect(nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SNAPSHOT))
      .rejects.toMatchObject({ status: 503 });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ data: { sql: 'DELETE FROM billing_usage_meters' } }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })));
    await expect(nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SEAT))
      .rejects.toMatchObject({ code: 'COMMERCIAL_OWNER_READ_RESPONSE_INVALID' });
  });
});
