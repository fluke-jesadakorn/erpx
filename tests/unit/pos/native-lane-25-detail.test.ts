import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativePosEvidenceDetail,
  type NativePosEvidenceDetailRow,
} from '../../../packages/server-pos/src/adapters/native-lane-25/detail-client';

const ENV = 'ERPX_NATIVE_POS_LANE25_CONFIG';
const previous = process.env[ENV];
const DEV = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co' },
    { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar', companyId: 'acme-co' },
    { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'acme-co' },
  ],
};
const config = () => JSON.stringify(DEV);
const actor = (overrides: Record<string, unknown> = {}) => ({
  holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co', branchId: 'br', ...overrides,
});
const refundFilters = { kind: 'refund' as const, id: 'r1', from: '2026-01-01', to: '2026-01-31', currency: 'USD' };
const paymentFilters = { kind: 'payment' as const, id: 'p1', from: '2026-01-01', to: '2026-01-31', currency: 'USD' };
const keys = {
  refund: [
    'source_id', 'sale_id', 'source_type', 'receipt_number', 'business_date',
    'accounting_date', 'currency', 'state', 'amount_minor', 'contribution_minor',
    'refunded_minor', 'revision', 'sale_version', 'branch_id',
  ],
  payment: [
    'source_id', 'source_type', 'sale_id', 'receipt_number', 'business_date',
    'accounting_date', 'currency', 'sale_state', 'state', 'method', 'amount_minor',
    'refunded_minor', 'contribution_minor', 'revision', 'sale_version', 'branch_id',
    'created_at',
  ],
};
const row = (kind: 'refund' | 'payment'): NativePosEvidenceDetailRow => {
  const result: Record<string, string | null> = {};
  for (const key of keys[kind])
    result[key] = key === 'source_type' ? kind
      : key === 'currency' ? 'USD'
      : key === 'business_date' ? '2026-01-02'
      : key === 'amount_minor' ? '100'
      : key === 'accounting_date' && kind === 'payment' ? null
      : 'v';
  return result;
};
const ok = (value: unknown) => new Response(JSON.stringify(value), {
  status: 200, headers: { 'content-type': 'application/json' },
});

afterEach(() => {
  vi.unstubAllGlobals();
  if (previous === undefined) delete process.env[ENV];
  else process.env[ENV] = previous;
});

describe('native POS evidence detail transport (lane 25 wave 2)', () => {
  it('forwards the actor Holding, Company and exact detail filters', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn().mockResolvedValue(ok({ row: row('refund') }));
    vi.stubGlobal('fetch', fetch);
    const result = await nativePosEvidenceDetail(actor(), refundFilters);
    expect(result).toEqual(row('refund'));
    const [url, init] = fetch.mock.calls[0] as [URL, RequestInit & { body: string; headers: Record<string, string> }];
    expect(url.toString()).toBe('http://127.0.0.1:3198/api/v1/holdings/acme/pos/evidence/detail');
    expect(init.method).toBe('POST');
    expect(init.headers['host']).toBe('acme.jesadakorn.com');
    expect(init.headers['origin']).toBe('https://acme.jesadakorn.com');
    expect(init.headers['x-erpx-context-company']).toBe('acme-co');
    expect(init.headers['x-erpx-workspace-branch']).toBe('br');
    expect(JSON.parse(init.body)).toEqual(refundFilters);
  });

  it('routes every registered Holding from the actor without a default', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn().mockImplementation(async () => ok({ row: row('payment') }));
    vi.stubGlobal('fetch', fetch);
    const cases = [
      ['h001', 'acme', 'acme.jesadakorn.com'],
      ['h002', 'northstar', 'northstar.jesadakorn.com'],
      ['h003', 'erpx', 'erpx-internal.jesadakorn.com'],
    ];
    for (const [index, [holdingId, holdingSlug, host]] of cases.entries()) {
      await expect(nativePosEvidenceDetail(actor({ holdingId, holdingSlug }), paymentFilters)).resolves.toEqual(row('payment'));
      const [url, init] = fetch.mock.calls[index] as [URL, RequestInit & { headers: Record<string, string> }];
      expect(url.toString()).toBe(`http://127.0.0.1:3198/api/v1/holdings/${holdingSlug}/pos/evidence/detail`);
      expect(init.headers['host']).toBe(host);
      expect(init.headers['origin']).toBe(`https://${host}`);
    }
    expect(fetch).toHaveBeenCalledTimes(3);
    const rejected = vi.fn();
    vi.stubGlobal('fetch', rejected);
    for (const bad of [actor({ holdingId: 'h404' }), actor({ holdingSlug: 'wrong' })])
      await expect(nativePosEvidenceDetail(bad, refundFilters)).rejects.toMatchObject({
        code: 'WORK_CONTEXT_CHANGED', status: 409,
      });
    expect(rejected).not.toHaveBeenCalled();
  });

  it('pins the exact 14/17-key row shapes and rejects malformed rows', async () => {
    process.env[ENV] = config();
    const client = (kind: 'refund' | 'payment', value: unknown) => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ row: value })));
      return nativePosEvidenceDetail(actor(), kind === 'refund' ? refundFilters : paymentFilters);
    };
    await expect(client('refund', row('refund'))).resolves.toEqual(row('refund'));
    await expect(client('payment', row('payment'))).resolves.toEqual(row('payment'));
    for (const bad of [
      { ...row('refund'), extra: true },
      { ...row('refund'), source_type: 'payment' },
      { ...row('refund'), amount_minor: 100 },
      { ...row('payment'), created_at: undefined },
      'nope',
    ])
      await expect(client('refund', bad)).rejects.toMatchObject({
        code: 'NATIVE_POS_LANE25_DETAIL_RESPONSE_INVALID', status: 503,
      });
    // An explicit null row is a valid "not found" response.
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ row: null })));
    await expect(nativePosEvidenceDetail(actor(), refundFilters)).resolves.toBeNull();
  });

  it('fails closed on bad requests, unconfigured transport and service errors', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    for (const bad of [
      { ...refundFilters, id: '' }, { ...refundFilters, currency: 'usd' },
      { ...refundFilters, kind: 'other' as never },
    ])
      await expect(nativePosEvidenceDetail(actor(), bad)).rejects.toMatchObject({
        code: 'NATIVE_POS_LANE25_DETAIL_REQUEST_INVALID', status: 422,
      });
    expect(fetch).not.toHaveBeenCalled();
    delete process.env[ENV];
    await expect(nativePosEvidenceDetail(actor(), refundFilters)).rejects.toMatchObject({
      code: 'NATIVE_POS_LANE25_CONFIGURATION_REQUIRED', status: 503,
    });
    process.env[ENV] = config();
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')));
    await expect(nativePosEvidenceDetail(actor(), refundFilters)).rejects.toMatchObject({
      code: 'NATIVE_POS_LANE25_DETAIL_UNAVAILABLE', status: 503,
    });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(
      JSON.stringify({ error: { code: 'NOT_FOUND', message: 'Gone.' } }),
      { status: 404, headers: { 'content-type': 'application/json' } },
    )));
    await expect(nativePosEvidenceDetail(actor(), refundFilters)).rejects.toMatchObject({
      code: 'NOT_FOUND', status: 404, message: 'Gone.',
    });
  });

  it('leaves no replaced TS SQL literal in the POS adapter', () => {
    const source = readFileSync(
      new URL('../../../packages/server-pos/src/adapters/pos.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('refundPaymentSumTextSql');
    expect(source).not.toContain("'refund' source_type,s.number receipt_number");
    expect(source).toContain('nativePosEvidenceDetail');
  });
});
