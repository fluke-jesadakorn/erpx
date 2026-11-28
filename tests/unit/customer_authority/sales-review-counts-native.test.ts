import { afterEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('@erpx/identity-access-server/public/authorization', () => ({
  requireWorkContext: vi.fn(async () => ({
    userId: 'user', holdingId: 'h', holdingSlug: 'acme', companyId: 'company',
    branchId: null, departmentId: null,
  })),
}));
vi.mock('@erpx/work-projection-ui/lib/destination-links', () => ({
  canonicalDestinationLinks: (value: unknown) => value,
}));
import {
  nativeSalesCustomerReviewCounts,
  nativeSalesCustomerReviewCountsGet,
} from '../../../modules/customer-authority/ui/src/lib/sales-review-counts';

const key = 'ERPX_NATIVE_CUSTOMER_AUTHORITY_READ_CONFIG';
const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44995',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', companyId: 'company' }],
});
const reviews = [
  { kind: 'identity', count: 2 },
  { kind: 'duplicates', count: 1 },
  { kind: 'access', count: 0 },
];
const request = () => new Request('https://acme.example.test/api/v1/holdings/acme/customer-reviews/counts');
const context = () => ({ params: Promise.resolve({ holdingSlug: 'acme' }) });

afterEach(() => {
  delete process.env[key];
  vi.unstubAllGlobals();
});

describe('native sales customer review counts bridge', () => {
  it('fails closed with 503 when the native configuration is absent', async () => {
    delete process.env[key];
    await expect(nativeSalesCustomerReviewCounts('acme')).rejects.toMatchObject({
      status: 503, code: 'CUSTOMER_SALES_REVIEW_COUNTS_UNAVAILABLE',
    });
    const response = await nativeSalesCustomerReviewCountsGet(request(), context());
    expect(response.status).toBe(503);
    expect((await response.json() as { error: { code: string } }).error.code).toBe('CUSTOMER_SALES_REVIEW_COUNTS_UNAVAILABLE');
  });

  it('reads the exact owner payload without preparing any SQL', async () => {
    process.env[key] = config;
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { reviews } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    expect(await nativeSalesCustomerReviewCounts('acme')).toEqual(reviews);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.toString()).toBe('http://127.0.0.1:44995/api/v1/holdings/acme/customer-reviews/counts');
    expect((init.headers as Record<string, string>)['x-erpx-context-company']).toBe('company');
    expect(init.body).toBeUndefined();
  });

  it('fails closed on a partial or open review kind set', async () => {
    process.env[key] = config;
    vi.stubGlobal('fetch', vi.fn(async () => new Response(
      JSON.stringify({ data: { reviews: [reviews[0], reviews[1]] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    )));
    await expect(nativeSalesCustomerReviewCounts('acme')).rejects.toMatchObject({
      status: 503, code: 'CUSTOMER_SALES_REVIEW_COUNTS_UNAVAILABLE',
    });
  });
});
