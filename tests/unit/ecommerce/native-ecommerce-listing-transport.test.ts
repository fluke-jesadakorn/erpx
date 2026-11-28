import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  nativeEcommerceListingAvailable,
  nativeEcommerceListingCommand,
  nativeEcommerceListingFailure,
  nativeEcommerceListingRequired,
  NativeEcommerceListingError,
} from '../../../modules/sales/ui/src/public/native-ecommerce-listing';
vi.mock('server-only', () => ({}));
import type { WorkContext } from '../../../modules/identity-access/server/src/public/authorization';

const config = () => JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44996',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
});
const actor = {
  userId: 'user', name: 'User', email: 'user@example.com', assignmentId: 'assignment',
  role: 'holding_owner', companyId: 'company', branchId: null, departmentId: null,
  holdingId: 'h001', holdingSlug: 'acme', holdingName: 'Acme',
  scope: { kind: 'holding', holdingSlug: 'acme' }, assignmentScope: 'holding', inherited: false,
  modules: ['ecommerce'], assignments: [], workspaces: [],
  authority: { revision: 7, assignmentVersion: 3 },
  iam: { permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'] },
} as unknown as WorkContext;
const context = {
  userId: 'user', holdingId: 'h001', companyId: 'company', branchId: null, departmentId: null,
  branchSlug: null, departmentSlug: null, assignmentId: 'assignment', assignmentVersion: 3,
  revision: 7, role: 'holding_owner', assignmentScopeKind: 'holding',
  modules: ['ecommerce'], permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'],
  expiresAt: 1_800_000_000_000,
};
const ok = (id: string, status = 'draft', version = 1) => new Response(JSON.stringify({
  data: { id, status, version }, context,
}), { status: 200, headers: { 'content-type': 'application/json' } });
const incoming = (overrides: Record<string, string> = {}) => new Headers({
  host: 'acme.example.com', origin: 'https://acme.example.com',
  cookie: '__Host-erpx-product=session', ...overrides,
});

afterEach(() => {
  delete process.env.ERPX_NATIVE_ECOMMERCE_LISTING_CONFIG;
  delete process.env.ERPX_NATIVE_ECOMMERCE_CATALOG_CONFIG;
  vi.restoreAllMocks();
});

describe('native Ecommerce listing command transport', () => {
  it('recognises exactly the owner-complete listing slice', () => {
    expect(nativeEcommerceListingAvailable()).toBe(false);
    expect(nativeEcommerceListingRequired({ entity: 'listing' })).toBe(true);
    expect(nativeEcommerceListingRequired({ entity: 'channel' })).toBe(false);
  });

  it('forwards a native listing create with exact fields and authority headers', async () => {
    process.env.ERPX_NATIVE_ECOMMERCE_LISTING_CONFIG = config();
    const transport = vi.fn().mockResolvedValue(ok('ecommerce-listing-key-1'));
    const result = await nativeEcommerceListingCommand(actor, 'acme', {
      entity: 'listing', channelId: 'channel-1', productId: 'product-1',
      title: '  Online product  ', currency: 'THB', price: '12.5',
    }, 'key-1', incoming(), transport);
    expect(result).toEqual({ id: 'ecommerce-listing-key-1', status: 'draft', version: 1 });
    const [method, url, headers, body] = transport.mock.calls[0]!;
    expect(method).toBe('POST');
    expect(url.toString()).toBe('http://127.0.0.1:44996/api/v1/holdings/acme/ecommerce/listings');
    expect(headers.get('origin')).toBe('https://acme.example.com');
    expect(headers.get('idempotency-key')).toBe('key-1');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
    expect(JSON.parse(body as string)).toEqual({
      channelId: 'channel-1', productId: 'product-1', title: '  Online product  ',
      currency: 'THB', price: '12.5',
    });
  });

  it('forwards a draft retitle to the exact change path', async () => {
    process.env.ERPX_NATIVE_ECOMMERCE_LISTING_CONFIG = config();
    const transport = vi.fn().mockResolvedValue(ok('listing-1', 'draft', 2));
    await nativeEcommerceListingCommand(actor, 'acme', {
      entity: 'listing', id: 'listing-1', title: 'Revised product', expectedVersion: 1,
    }, 'key-2', incoming(), transport);
    const [, url, , body] = transport.mock.calls[0]!;
    expect(url.toString()).toBe(
      'http://127.0.0.1:44996/api/v1/holdings/acme/ecommerce/listings/listing-1/changes',
    );
    expect(JSON.parse(body as string)).toEqual({ expectedVersion: 1, title: 'Revised product' });
  });

  it('forwards a listing review decision to the native review port', async () => {
    process.env.ERPX_NATIVE_ECOMMERCE_LISTING_CONFIG = config();
    const transport = vi.fn().mockResolvedValue(ok('listing-1', 'published', 2));
    await nativeEcommerceListingCommand(actor, 'acme', {
      entity: 'listing', id: 'listing-1', action: 'publish', expectedVersion: 1,
      reason: 'Independent review of the exact unit price',
    }, 'key-3', incoming(), transport);
    const [, url, , body] = transport.mock.calls[0]!;
    expect(url.toString()).toBe('http://127.0.0.1:44996/api/v1/holdings/acme/ecommerce/reviews');
    expect(JSON.parse(body as string)).toEqual({
      kind: 'listing', id: 'listing-1', action: 'publish', expectedVersion: 1,
      reason: 'Independent review of the exact unit price',
    });
  });

  it('returns undefined outside the slice and fails closed on a malformed receipt', async () => {
    process.env.ERPX_NATIVE_ECOMMERCE_LISTING_CONFIG = config();
    const transport = vi.fn();
    expect(await nativeEcommerceListingCommand(actor, 'acme', {
      entity: 'cart', action: 'abandon',
    }, 'key-4', incoming(), transport)).toBeUndefined();
    expect(transport).not.toHaveBeenCalled();
    const bad = vi.fn().mockResolvedValue(ok('wrong-id'));
    await expect(nativeEcommerceListingCommand(actor, 'acme', {
      entity: 'listing', channelId: 'channel-1', productId: 'product-1',
      title: 'Product', currency: 'THB', price: '1',
    }, 'key-5', incoming(), bad)).rejects.toBeInstanceOf(NativeEcommerceListingError);
    expect(nativeEcommerceListingFailure(new NativeEcommerceListingError('X', 422))).toMatchObject({
      status: 422,
    });
  });
});
