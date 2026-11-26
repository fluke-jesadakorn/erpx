import { readFileSync } from 'node:fs';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import type { WorkContext } from '@erpx/server-identity_access/domain/authorization';
import { nativeFinanceAging } from '../../../packages/server-finance/src/public/finance-invoice-read';

const routePath = new URL(
  '../../../packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/finance/[...path]/route.ts',
  import.meta.url,
);
const actor = {
  userId: 'reader',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company-1',
} as WorkContext;

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44997',
  timeoutMs: 1000,
  holdings: [
    {
      origin: 'https://acme.example.test',
      host: 'acme.example.test',
      holdingId: 'h001',
      holdingSlug: 'acme',
    },
  ],
});

/** The complete row the Rust Axum handler projects; no SQL plan is returned. */
const rustRow = {
  id: 'invoice-1',
  company_id: 'company-1',
  party_id: 'party-1',
  party_code: 'C-1',
  party_name: 'Customer',
  due_date: '2026-01-01',
  currency: 'THB',
  historical_outstanding_minor: 125,
  historical_base_outstanding_minor: 4500,
  outstanding_minor: 125,
  base_outstanding_minor: 4500,
  outstanding: '1.25',
  baseOutstanding: '45.00',
  bucket: '31-60',
  as_of: '2026-02-01',
  asOf: '2026-02-01',
  legacy_date_evidence: false,
  source_id: null,
};

const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json' },
  });

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

describe('finance catch-all aging cutover', () => {
  it('removes the TypeScript owner execution from the catch-all branch', () => {
    const lines = readFileSync(routePath, 'utf8').split('\n');
    const branch = lines.find((line) => line.includes('parts[0]') && line.includes('"aging"'));
    expect(branch).toBeDefined();
    expect(branch).toContain('nativeFinanceAging(');
    expect(branch).not.toMatch(/await aging\(|financeReadService|agingBalances|purchasedFinanceRead/);
    expect(branch).not.toMatch(/\bSELECT\b|\bINSERT\b|\bUPDATE\b|\bDELETE\b/);
    const ownerImport = lines.find((line) => line.includes('@erpx/server-finance/public/finance\''));
    expect(ownerImport).toBeDefined();
    expect(ownerImport).not.toMatch(/\baging,/);
  });

  it('executes the configured native Rust aging route and returns its full payload unchanged', async () => {
    vi.stubEnv('ERPX_NATIVE_FINANCE_LANE_2_CONFIG', config);
    const calls: { url: string; headers: HeadersInit | undefined }[] = [];
    vi.stubGlobal('fetch', async (url: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(url), headers: init?.headers });
      return json({ data: [rustRow] });
    });
    const rows = await nativeFinanceAging(actor, 'payable', '2026-02-01', 'chart-1');
    expect(rows).toEqual([rustRow]);
    // HTTP parity: every legacy derived field arrives from the Rust owner
    // untouched; the TypeScript caller performs no SQL and no derivation.
    for (const field of [
      'id',
      'company_id',
      'party_id',
      'party_code',
      'party_name',
      'due_date',
      'currency',
      'historical_outstanding_minor',
      'historical_base_outstanding_minor',
      'outstanding_minor',
      'base_outstanding_minor',
      'outstanding',
      'baseOutstanding',
      'bucket',
      'asOf',
      'legacy_date_evidence',
      'source_id',
    ] as const) {
      expect(rows[0]).toHaveProperty(field, rustRow[field]);
    }
    expect(rows[0]).toMatchObject({
      outstanding: '1.25',
      baseOutstanding: '45.00',
      bucket: '31-60',
      asOf: '2026-02-01',
    });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url).toBe(
      'http://127.0.0.1:44997/api/v1/holdings/acme/finance/native-lane-2/aging?kind=payable&asOf=2026-02-01&chartId=chart-1',
    );
    expect(calls[0]!.headers).toMatchObject({
      host: 'acme.example.test',
      origin: 'https://acme.example.test',
      'x-erpx-context-company': 'company-1',
    });
  });

  it('fails closed on an invalid cutoff before any native read', async () => {
    vi.stubEnv('ERPX_NATIVE_FINANCE_LANE_2_CONFIG', config);
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(nativeFinanceAging(actor, 'receivable', 'bad-date')).rejects.toMatchObject({
      code: 'INVALID_AS_OF',
      status: 422,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('fails closed when the native service rejects the request', async () => {
    vi.stubEnv('ERPX_NATIVE_FINANCE_LANE_2_CONFIG', config);
    vi.stubGlobal('fetch', async () => json({ error: { code: 'DENIED' } }, 503));
    await expect(nativeFinanceAging(actor, 'receivable', '2026-02-01')).rejects.toMatchObject({
      code: 'FINANCE_NATIVE_LANE_2_UNAVAILABLE',
      status: 503,
    });
  });
});
