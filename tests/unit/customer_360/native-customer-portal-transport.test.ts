import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeCustomerPortalClient,
  nativeCustomerPortalConfiguration,
  NativeCustomerPortalError,
} from '../../../packages/server-customer_360/src/adapters/native-customer-portal';

const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44997',
    timeoutMs: 1000,
    holdings: [
      { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
    ],
    ...overrides,
  });
const incoming = new Headers({
  host: 'acme.example.com',
  cookie: '__Host-erpx-product=session',
  origin: 'https://acme.example.com',
  'x-erpx-context-company': 'company',
  'x-erpx-workspace-branch': 'main',
  'x-erpx-untrusted-header': 'ignored',
});
const item = {
  binding_id: 'binding',
  binding_version: 2,
  order_id: 'order:1',
  number: 'SO-1',
  kind: 'sales',
  state: 'confirmed',
  due_date: '2026-12-01',
  currency: 'USD',
  updated_at: 100,
  fulfillments: [],
  actions: [],
  sharedFiles: [],
};
const projection = { items: [item], projectionVersion: 'customer-order-v1' };
const outcome = {
  id: 'customer-portal-action-1',
  bindingId: 'binding',
  orderId: 'order:1',
  action: 'acknowledge_delivery',
  status: 'recorded',
  createdAt: 100,
};
const ok = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json' },
  });

describe('native customer-portal transport', () => {
  it('accepts one bounded private Holding port and rejects unsafe configuration', () => {
    expect(() => nativeCustomerPortalConfiguration(config())).not.toThrow();
    for (const overrides of [
      { privateBaseUrl: 'http://127.0.0.1:3100' },
      { privateBaseUrl: 'http://127.0.0.1:3101' },
      { privateBaseUrl: 'http://127.0.0.1:3201' },
      { privateBaseUrl: 'http://localhost:44997' },
      { privateBaseUrl: 'http://127.0.0.1:44997/path' },
      { timeoutMs: 100 },
      { holdings: [] },
      { holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }] },
      { holdings: [{ origin: 'https://acme.example.com/path', holdingSlug: 'acme', holdingId: 'h001' }] },
      { holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'Acme', holdingId: 'h001' }] },
      {
        holdings: [
          { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
          { origin: 'https://other.example.com', holdingSlug: 'acme', holdingId: 'h002' },
        ],
      },
    ])
      expect(() => nativeCustomerPortalConfiguration(config(overrides))).toThrow(
        NativeCustomerPortalError,
      );
  });

  it('forwards only bounded session and Holding authority on a projection read', async () => {
    const send = vi.fn().mockResolvedValue(ok({ data: projection }));
    const result = await createNativeCustomerPortalClient(config(), send).projection(
      incoming,
      'acme',
    );
    expect(result).toEqual(projection);
    const [url, init] = send.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44997/api/v1/holdings/acme/customer-portal',
    );
    expect(init.method).toBe('GET');
    const headers = init.headers as Headers;
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('origin')).toBe('https://acme.example.com');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-workspace-branch')).toBe('main');
    expect(headers.get('x-erpx-untrusted-header')).toBeNull();
  });

  it('binds an exact idempotent command to the private Holding target', async () => {
    const send = vi.fn().mockResolvedValue(ok({ data: outcome }, 201));
    const input = { bindingId: 'binding', expectedBindingVersion: 2, action: 'acknowledge_delivery' };
    const result = await createNativeCustomerPortalClient(config(), send).command(
      incoming,
      'acme',
      input,
      'command-key-1',
    );
    expect(result).toEqual(outcome);
    const [url, init] = send.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44997/api/v1/holdings/acme/customer-portal',
    );
    expect(init.method).toBe('POST');
    expect(JSON.parse(String(init.body))).toEqual(input);
    const headers = init.headers as Headers;
    expect(headers.get('content-type')).toBe('application/json');
    expect(headers.get('idempotency-key')).toBe('command-key-1');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  });

  it('rejects an unregistered host or a body over the bounded limit before transport', async () => {
    const send = vi.fn().mockResolvedValue(ok({ data: projection }));
    const client = createNativeCustomerPortalClient(config(), send);
    await expect(
      client.projection(new Headers({ host: 'foreign.example.com' }), 'acme'),
    ).rejects.toMatchObject({ code: 'NATIVE_CUSTOMER_PORTAL_AUTHORITY_INVALID' });
    await expect(client.projection(incoming, 'other')).rejects.toMatchObject({
      code: 'NATIVE_CUSTOMER_PORTAL_AUTHORITY_INVALID',
    });
    await expect(
      client.command(incoming, 'acme', { note: 'x'.repeat(50001) }, 'command-key-1'),
    ).rejects.toMatchObject({ code: 'NATIVE_CUSTOMER_PORTAL_BODY_LIMIT', status: 413 });
    expect(send).not.toHaveBeenCalled();
  });

  it('surfaces bounded owner errors and rejects malformed payloads', async () => {
    const denied = createNativeCustomerPortalClient(
      config(),
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({ error: { code: 'CUSTOMER_PORTAL_BINDING_EXPIRED', message: 'Expired.' } }),
          { status: 403, headers: { 'content-type': 'application/json' } },
        ),
      ),
    );
    await expect(denied.projection(incoming, 'acme')).rejects.toMatchObject({
      code: 'CUSTOMER_PORTAL_BINDING_EXPIRED',
      status: 403,
    });

    const malformed = createNativeCustomerPortalClient(
      config(),
      vi.fn().mockResolvedValue(ok({ data: { ...projection, projectionVersion: 'wrong' } })),
    );
    await expect(malformed.projection(incoming, 'acme')).rejects.toMatchObject({
      code: 'NATIVE_CUSTOMER_PORTAL_RESPONSE_INVALID',
    });

    const unavailable = createNativeCustomerPortalClient(
      config(),
      vi.fn().mockRejectedValue(new Error('offline')),
    );
    await expect(unavailable.projection(incoming, 'acme')).rejects.toMatchObject({
      code: 'NATIVE_CUSTOMER_PORTAL_UNAVAILABLE',
      status: 503,
    });
  });
});
