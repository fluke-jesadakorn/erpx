import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

vi.mock('server-only', () => ({}));

import {
  nativeCustomerRead,
  nativeCustomerReadConfigured,
  FINANCE_LANE5_RD_CUSTOMER_DIRECTORY,
} from '../../../packages/server-finance/src/adapters/native-lane-5/client';
import {
  salesPricingCustomers,
  salesPricingCustomerValid,
  salesCustomerProfile,
  salesCustomerDirectory,
  salesNoteCustomer,
} from '../../../packages/server-finance/src/adapters/sales-customer-sources';

const ENV = 'ERPX_NATIVE_FINANCE_CUSTOMER_READ_CONFIG';
const root = fileURLToPath(new URL('../../../', import.meta.url));
const holdings = [
  {
    origin: 'https://acme.jesadakorn.com',
    host: 'acme.jesadakorn.com',
    holdingId: 'h001',
    holdingSlug: 'acme',
    companyId: 'company-1',
  },
  {
    origin: 'https://northstar.jesadakorn.com',
    host: 'northstar.jesadakorn.com',
    holdingId: 'h002',
    holdingSlug: 'northstar',
    companyId: 'company-2',
  },
  {
    origin: 'https://erpx-internal.jesadakorn.com',
    host: 'erpx-internal.jesadakorn.com',
    holdingId: 'h003',
    holdingSlug: 'erpx',
    companyId: 'company-3',
  },
];
const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44992',
    timeoutMs: 1000,
    holdings,
    ...overrides,
  });
const actor = {
  userId: 'u1',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company-1',
  branchId: null,
};
const db = {} as never;
const ok = (kind: string, value: unknown) =>
  new Response(JSON.stringify({ kind, value }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
const customer = { id: 'party-1', code: 'C001', name: 'Acme', currency: 'THB' };
const page = {
  items: [{ ...customer, page_key: 'Acme' }],
  nextCursor: 'eyJrZXkiOiJBY21lIn0',
  previousCursor: null,
  hasMore: true,
};

describe('native Finance lane 5 customer read transport', () => {
  beforeEach(() => {
    process.env[ENV] = config();
  });
  afterEach(() => {
    delete process.env[ENV];
    vi.unstubAllGlobals();
  });

  it('forwards the registered read with exact Holding and company isolation', async () => {
    const fetchMock = vi.fn().mockResolvedValue(ok('pricing_customers', [customer]));
    vi.stubGlobal('fetch', fetchMock);
    const result = await salesPricingCustomers(db, actor as never);
    expect(result.results).toEqual([customer]);
    expect(result.success).toBe(true);
    const [url, init] = fetchMock.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44992/api/v1/holdings/acme/finance/customer-read',
    );
    const headers = new Headers(init.headers as HeadersInit);
    expect(headers.get('host')).toBe('acme.jesadakorn.com');
    expect(headers.get('origin')).toBe('https://acme.jesadakorn.com');
    expect(headers.get('x-erpx-context-company')).toBe('company-1');
    expect(headers.get('x-erpx-context-branch')).toBeNull();
    expect(JSON.parse(String(init.body))).toEqual({
      kind: 'pricing_customers',
      id: null,
      currency: null,
      search: null,
      status: null,
      cursor: null,
      version: null,
    });
  });

  it('returns the scalar, single-row, page and note shapes unchanged', async () => {
    const responses: Response[] = [
      ok('pricing_customer_valid', true),
      ok('customer_profile', customer),
      ok(FINANCE_LANE5_RD_CUSTOMER_DIRECTORY, page),
      ok('note_customer', { id: 'party-1', name: 'Acme', code: 'C001', version: 3 }),
    ];
    vi.stubGlobal('fetch', vi.fn().mockImplementation(() => Promise.resolve(responses.shift())));
    expect(await salesPricingCustomerValid(db, actor as never, 'party-1', 'THB')).toBe(true);
    expect(await salesCustomerProfile(db, actor as never, 'party-1')).toEqual(customer);
    expect(await salesCustomerDirectory(db, actor as never, 'A', 'active')).toEqual(page);
    expect(await salesNoteCustomer(db, actor as never, 'party-1')).toEqual({
      id: 'party-1',
      name: 'Acme',
      code: 'C001',
      version: 3,
    });
    const [, directoryInit] = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls[2] as [URL, RequestInit];
    expect(JSON.parse(String(directoryInit.body))).toMatchObject({
      kind: FINANCE_LANE5_RD_CUSTOMER_DIRECTORY,
      search: 'A',
      status: 'active',
    });
  });

  it('fails closed when configuration is absent', async () => {
    delete process.env[ENV];
    expect(nativeCustomerReadConfigured()).toBe(false);
    await expect(salesCustomerProfile(db, actor as never, 'party-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_REQUIRED',
    });
    process.env[ENV] = config();
    expect(nativeCustomerReadConfigured()).toBe(true);
  });

  it('fails closed on a malformed native response', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ kind: 'pricing_customers', value: [{ id: 'x' }] }), { status: 200 }),
    ));
    await expect(salesPricingCustomers(db, actor as never)).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_FINANCE_CUSTOMER_READ_RESPONSE_INVALID',
    });
  });

  it('fails closed on a native 5xx and propagates a native 4xx', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status: 500 })));
    await expect(salesCustomerProfile(db, actor as never, 'party-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_FINANCE_CUSTOMER_READ_UNAVAILABLE',
    });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ error: { code: 'FINANCE_CUSTOMER_READ_REQUEST_INVALID', message: 'bad' } }), { status: 422 }),
    ));
    await expect(salesCustomerProfile(db, actor as never, 'party-1')).rejects.toMatchObject({
      status: 422,
      code: 'FINANCE_CUSTOMER_READ_REQUEST_INVALID',
    });
  });

  it('routes every registered Holding to its own host and company', async () => {
    const fetchMock = vi.fn().mockImplementation(() => Promise.resolve(ok('pricing_customers', [customer])));
    vi.stubGlobal('fetch', fetchMock);
    for (const holding of holdings) {
      await nativeCustomerRead('pricing_customers', {
        ...actor,
        holdingId: holding.holdingId,
        holdingSlug: holding.holdingSlug,
        companyId: holding.companyId,
      });
      const [url, init] = fetchMock.mock.calls.at(-1) as [URL, RequestInit];
      const headers = new Headers(init.headers as HeadersInit);
      expect(url.toString()).toBe(
        `http://127.0.0.1:44992/api/v1/holdings/${holding.holdingSlug}/finance/customer-read`,
      );
      expect(headers.get('host')).toBe(holding.host);
      expect(headers.get('origin')).toBe(holding.origin);
      expect(headers.get('x-erpx-context-company')).toBe(holding.companyId);
    }
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it('rejects a Holding that is not registered before any request', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    await expect(
      nativeCustomerRead('pricing_customers', { ...actor, holdingId: 'other' }),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    await expect(
      nativeCustomerRead('pricing_customers', { ...actor, holdingSlug: 'unknown' }),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('rejects a company that does not match the selected Holding', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    await expect(
      nativeCustomerRead('pricing_customers', { ...actor, holdingId: 'h002', holdingSlug: 'northstar' }),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('accepts the exact development environment configuration', async () => {
    process.env[ENV] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3198',
      timeoutMs: 10000,
      holdings: [
        { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
        { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
        { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
      ],
    });
    const fetchMock = vi.fn().mockImplementation(() => Promise.resolve(ok('pricing_customers', [customer])));
    vi.stubGlobal('fetch', fetchMock);
    await expect(nativeCustomerRead('pricing_customers', { ...actor, holdingId: 'h003', holdingSlug: 'erpx' })).resolves.toEqual([customer]);
    const [url, init] = fetchMock.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).toBe('http://127.0.0.1:3198/api/v1/holdings/erpx/finance/customer-read');
    expect(new Headers(init.headers as HeadersInit).get('host')).toBe('erpx-internal.jesadakorn.com');
  });

  it('fails closed on a config whose host is missing or disagrees with origin', async () => {
    const withoutHost = holdings.map(({ host: _host, ...rest }) => rest);
    process.env[ENV] = config({ holdings: withoutHost });
    await expect(salesCustomerProfile(db, actor as never, 'party-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID',
    });
    process.env[ENV] = config({
      holdings: [{ ...holdings[0], host: 'evil.example.com' }],
    });
    await expect(salesCustomerProfile(db, actor as never, 'party-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID',
    });
  });
});

describe('native Finance lane 5 SQL/result oracle', () => {
  it('replaced TS reads contain no SQL and delegate to the native client', () => {
    const src = readFileSync(
      new URL('../../../packages/server-finance/src/adapters/sales-customer-sources.ts', import.meta.url),
      'utf8',
    );
    expect(src).toContain("from './native-lane-5/client'");
    expect(src).not.toMatch(/\bSELECT\b/);
    expect(src).not.toMatch(/\.prepare\(/);
    expect(src).not.toMatch(/readListPage/);
  });

  it('pins the byte-identical statement heads executed by Rust', () => {
    const domain = readFileSync(
      new URL('../../../packages/server-finance/src/adapters/native_lane_5/domain.rs', import.meta.url),
      'utf8',
    );
    const oracles: Record<string, string> = {
      PRICING_CUSTOMERS_SQL:
        'e1476d8f12e47f331c5ce8dfe50ce29cc4552657d4975c4d3f23c6fbd0481df6',
      PRICING_CUSTOMER_VALID_SQL:
        '04bbf6a2022aaf69a2d88424e831430d6308263d9b56890fda9942a0f206d391',
      CUSTOMER_PROFILE_SQL:
        'cdde093d36fa76d24922ecac74dc6e2a6993aba8d42c72b7b30b9c71dbffff78',
      CUSTOMER_DIRECTORY_SQL:
        '047b48e0be125e3704d0990121ade0773b4aff840d83e1fb830a86733e9732a0',
      NOTE_CUSTOMER_SQL:
        'b7d66bcfdf8b5941db80ff2509a369f2d9d644ca0b3a98d0387cb77a527451c0',
    };
    for (const [name, expected] of Object.entries(oracles)) {
      const match = domain.match(new RegExp(`pub const ${name}: &str = "([^"]+)"`));
      expect(match, name).not.toBeNull();
      expect(createHash('sha256').update(match![1]).digest('hex')).toBe(expected);
    }
    expect(root).toContain('erpx');
  });
});
