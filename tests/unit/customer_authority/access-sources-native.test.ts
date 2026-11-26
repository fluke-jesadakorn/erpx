import { afterEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-identity_access/public/authorization', () => ({
  requireWorkContext: vi.fn(async () => ({
    userId: 'user', holdingId: 'h', holdingSlug: 'acme', companyId: 'company',
    branchId: null, departmentId: null,
  })),
}));
vi.mock('@erpx/feature-work_projection/lib/destination-links', () => ({
  canonicalDestinationLinks: (value: unknown) => value,
}));
import {
  nativeCustomerAccessSources,
  nativeCustomerAccessSourcesGet,
} from '../../../packages/feature-customer_authority/src/lib/access-sources';

const key = 'ERPX_NATIVE_CUSTOMER_AUTHORITY_READ_CONFIG';
const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44994',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', companyId: 'company' }],
});
const request = () => new Request('https://acme.example.test/api/v1/holdings/acme/customer-access');
const context = () => ({ params: Promise.resolve({ holdingSlug: 'acme' }) });

afterEach(() => {
  delete process.env[key];
  vi.unstubAllGlobals();
});

describe('native customer-access sources bridge', () => {
  it('fails closed with 503 when the native configuration is absent', async () => {
    delete process.env[key];
    await expect(nativeCustomerAccessSources('acme')).rejects.toMatchObject({
      status: 503, code: 'CUSTOMER_ACCESS_SOURCES_UNAVAILABLE',
    });
    const response = await nativeCustomerAccessSourcesGet(request(), context());
    expect(response.status).toBe(503);
    expect((await response.json() as { error: { code: string } }).error.code).toBe('CUSTOMER_ACCESS_SOURCES_UNAVAILABLE');
  });

  it('reads the exact owner payload without preparing any SQL', async () => {
    process.env[key] = config;
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { sources: [{ id: 'order-1', number: 'SO-1', name: 'Acme', version: 2 }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    expect(await nativeCustomerAccessSources('acme')).toEqual([
      { id: 'order-1', number: 'SO-1', name: 'Acme', version: 2 },
    ]);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/customer-access/sources');
    expect((init.headers as Record<string, string>)['x-erpx-context-company']).toBe('company');
    expect(init.body).toBeUndefined();
  });

  it('fails closed on a partial source row instead of returning a projection', async () => {
    process.env[key] = config;
    vi.stubGlobal('fetch', vi.fn(async () => new Response(
      JSON.stringify({ data: { sources: [{ id: 'order-1' }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    )));
    await expect(nativeCustomerAccessSources('acme')).rejects.toMatchObject({
      status: 503, code: 'CUSTOMER_ACCESS_SOURCES_UNAVAILABLE',
    });
  });
});
