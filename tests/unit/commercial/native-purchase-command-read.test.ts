import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import {
  nativeCommercialOwnerRead,
  COMMERCIAL_RD_PURCHASE_FACTS,
} from '../../../packages/server-commercial/src/adapters/native-owner-read';

const src = (name: string) => readFileSync(
  resolve(__dirname, '../../../packages/server-commercial/src', name), 'utf8',
);
const factsSql = 'SELECT grants,history FROM company_purchase_facts';

describe('native purchase-command facts read', () => {
  const envKeys = [
    'ERPX_NATIVE_COMMERCIAL_OWNER_READ_CONFIG',
    'ERPX_NATIVE_COMMERCIAL_CONFIG',
    'ERPX_NATIVE_BILLING_CONFIG',
    'ERPX_NATIVE_COMMERCIAL_OWNER_WRITE_CONFIG',
  ] as const;
  const originals = new Map(envKeys.map((key) => [key, process.env[key]]));
  const envKey = envKeys[0];
  beforeEach(() => {
    process.env[envKey] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44993',
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

  it('requests the owner-planned purchase.facts SELECT with no SQL from the caller', async () => {
    const kinds: string[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      kinds.push((JSON.parse(String(init.body)) as { kind: string }).kind);
      return new Response(JSON.stringify({ data: { sql: factsSql } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }));
    expect(await nativeCommercialOwnerRead(COMMERCIAL_RD_PURCHASE_FACTS)).toBe(factsSql);
    expect(kinds).toEqual(['purchase.facts']);
  });

  it('fails closed without native configuration and never falls back to TS SQL', async () => {
    for (const key of envKeys) delete process.env[key];
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(nativeCommercialOwnerRead(COMMERCIAL_RD_PURCHASE_FACTS))
      .rejects.toMatchObject({ code: 'COMMERCIAL_OWNER_READ_UNAVAILABLE', status: 503 });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ data: { sql: 'DELETE FROM company_package_grants' } }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })));
    await expect(nativeCommercialOwnerRead(COMMERCIAL_RD_PURCHASE_FACTS))
      .rejects.toMatchObject({ code: 'COMMERCIAL_OWNER_READ_UNAVAILABLE', status: 503 });
  });

  it('moves the purchase-facts SQL authority out of the TS adapter', () => {
    const adapter = src('adapters/package-grants.ts');
    expect(adapter).toContain('nativeCommercialOwnerRead(COMMERCIAL_RD_PURCHASE_FACTS)');
    expect(adapter).not.toContain('purchaseFactAgg');
    expect(adapter).not.toContain('jsonb_agg(to_jsonb(g)');
    expect(adapter).not.toContain('company_package_history_rights h');
    // The native domain registry owns the exact statement and its two binds.
    const domain = src('domain/owner_read.rs');
    expect(domain).toContain('pub const PURCHASE_FACTS: &str = "purchase.facts"');
    expect(domain).toContain('pub const SQL_PURCHASE_FACTS');
    expect(domain).toContain('PURCHASE_FACTS => Some(SQL_PURCHASE_FACTS)');
  });

  it('keeps the purchase-command boundary on the native-backed facts reader', () => {
    const boundary = src('adapters/purchase-command-boundary.ts');
    expect(boundary).toContain('readCompanyPurchaseFacts');
    expect(boundary).not.toContain('COALESCE((SELECT jsonb_agg(to_jsonb(g)');
    expect(boundary).not.toContain('nativeCommercialOwnerRead'); // the transport stays behind the adapter
  });
});
