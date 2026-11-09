import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeCustomerActivityClient,
  nativeCustomerActivityConfiguration,
} from '../../../packages/server-customer_360/src/adapters/native-executive-customer-activity';

const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44996',
    timeoutMs: 1000,
    holdings: [
      { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
    ],
    ...overrides,
  });
const incoming = new Headers({
  host: 'acme.example.com',
  cookie: '__Host-erpx-product=session',
});
const actor = {
  userId: 'user',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
} as never;
const row = {
  id: 'order:order-1',
  source_id: 'order-1',
  source_type: 'order',
  title: 'sales · SO-100',
  status: 'confirmed',
  version: 1,
  at: 100,
  href: '/commerce/orders/order-1',
  customer_id: 'customer',
  customer_name: 'Acme Customer',
};
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

describe('native Executive customer-activity transport', () => {
  it('accepts one bounded private Holding port and rejects unsafe configuration', () => {
    expect(() => nativeCustomerActivityConfiguration(config())).not.toThrow();
    for (const overrides of [
      { privateBaseUrl: 'http://127.0.0.1:3100' },
      { privateBaseUrl: 'http://localhost:44996' },
      { privateBaseUrl: 'http://127.0.0.1:44996/path' },
      { holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }] },
      { holdings: [{ origin: 'https://acme.example.com/path', holdingSlug: 'acme', holdingId: 'h001' }] },
      { holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'Acme', holdingId: 'h001' }] },
      { timeoutMs: 100 },
    ])
      expect(() => nativeCustomerActivityConfiguration(config(overrides))).toThrow();
  });

  it('forwards the bounded search, session cookie and Holding authority', async () => {
    const send = vi.fn().mockResolvedValue(ok({ data: { rows: [row] } }));
    const client = createNativeCustomerActivityClient(config(), send);
    const result = await client.search(incoming, actor, {
      from: 10,
      to: 20,
      query: 'Acme',
      limit: 5,
      cursorAt: 15,
      cursorId: 'order:order-1',
    });
    expect(result).toEqual([row]);
    const [url, headers] = send.mock.calls[0];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44996/api/v1/holdings/acme/customers/activity?from=10&to=20&query=Acme&limit=5&cursorAt=15&cursorId=order%3Aorder-1',
    );
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('origin')).toBe('https://acme.example.com');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  });

  it('reads one exact record and returns null when the native page is empty', async () => {
    const send = vi.fn().mockResolvedValue(ok({ data: { rows: [row] } }));
    const client = createNativeCustomerActivityClient(config(), send);
    expect(
      await client.read(incoming, actor, 'order:order-1', { from: 10, to: 20 }),
    ).toEqual(row);
    const [url] = send.mock.calls[0];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44996/api/v1/holdings/acme/customers/activity?from=10&to=20&recordId=order%3Aorder-1&limit=1',
    );
    const empty = createNativeCustomerActivityClient(
      config(),
      vi.fn().mockResolvedValue(ok({ data: { rows: [] } })),
    );
    expect(
      await empty.read(incoming, actor, 'order:order-1', { from: 10, to: 20 }),
    ).toBeNull();
  });

  it('surfaces the bounded owner error and rejects malformed rows', async () => {
    const error = createNativeCustomerActivityClient(
      config(),
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            error: { code: 'WORK_CONTEXT_CHANGED', message: 'Reload.' },
          }),
          { status: 409, headers: { 'content-type': 'application/json' } },
        ),
      ),
    );
    await expect(
      error.search(incoming, actor, { from: 10, to: 20, query: '', limit: 5 }),
    ).rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });

    const malformed = createNativeCustomerActivityClient(
      config(),
      vi.fn().mockResolvedValue(ok({ data: { rows: [{ ...row, at: -1 }] } })),
    );
    await expect(
      malformed.search(incoming, actor, { from: 10, to: 20, query: '', limit: 5 }),
    ).rejects.toMatchObject({ code: 'NATIVE_CUSTOMER_ACTIVITY_RESPONSE_INVALID' });

    const foreign = createNativeCustomerActivityClient(
      config(),
      vi.fn().mockResolvedValue(ok({ data: { rows: [row] } })),
    );
    await expect(
      foreign.search(new Headers({ host: 'other.example.com' }), actor, {
        from: 10,
        to: 20,
        query: '',
        limit: 5,
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_CUSTOMER_ACTIVITY_AUTHORITY_INVALID' });
  });
});
