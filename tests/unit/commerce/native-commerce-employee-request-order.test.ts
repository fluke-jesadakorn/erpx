import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  NativeCommerceEmployeeOrderError,
  nativeCommerceEmployeeOrderReader,
} from '../../../modules/commerce/ui/src/public/native-commerce-employee-request-order';
import type { NativeCommerceOrdersTransport } from '../../../modules/commerce/ui/src/public/native-commerce-orders';

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44994',
  host: 'acme.example.com',
  holdingSlug: 'acme',
  origin: 'https://acme.example.com',
  timeoutMs: 1000,
});
const order = {
  kind: 'purchase',
  company_id: 'co-1',
  branch_id: 'br-1',
  department_id: null,
  state: 'approved',
  version: 3,
};
const response = (body: unknown) => new Response(JSON.stringify(body), {
  status: 200,
  headers: { 'content-type': 'application/json' },
});
const original = process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;

beforeEach(() => {
  process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config;
});
afterEach(() => {
  if (original === undefined) delete process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
  else process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = original;
});

describe('native Commerce employee-request order transport', () => {
  it('accepts the complete six-field native payload', async () => {
    const transport: NativeCommerceOrdersTransport = vi.fn(async () => response({ data: order }));
    const reader = nativeCommerceEmployeeOrderReader(transport);
    await expect(reader.read('co-1', 'order-1')).resolves.toEqual(order);
    const [method, url, headers] = vi.mocked(transport).mock.calls[0]!;
    expect(method).toBe('GET');
    expect(url.pathname).toBe('/api/v1/holdings/acme/commerce/employee-request-order');
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('x-erpx-context-company')).toBe('co-1');
    expect(headers.get('x-erpx-context-record')).toBe('order-1');
  });

  it('maps a missing row to null without a fallback', async () => {
    const transport: NativeCommerceOrdersTransport = vi.fn(async () => response({ data: null }));
    const reader = nativeCommerceEmployeeOrderReader(transport);
    await expect(reader.read('co-1', 'order-1')).resolves.toBeNull();
    expect(vi.mocked(transport)).toHaveBeenCalledTimes(1);
  });

  it('fails closed on a partial or renamed payload', async () => {
    const partial = { ...order };
    delete (partial as Record<string, unknown>).version;
    const transport: NativeCommerceOrdersTransport = vi.fn(async () => response({ data: partial }));
    const reader = nativeCommerceEmployeeOrderReader(transport);
    await expect(reader.read('co-1', 'order-1'))
      .rejects.toBeInstanceOf(NativeCommerceEmployeeOrderError);
  });

  it('fails closed on non-200 and non-JSON replies', async () => {
    const unavailable = nativeCommerceEmployeeOrderReader(async () =>
      new Response('{}', { status: 503, headers: { 'content-type': 'application/json' } }));
    await expect(unavailable.read('co-1', 'order-1'))
      .rejects.toBeInstanceOf(NativeCommerceEmployeeOrderError);
    const invalid = nativeCommerceEmployeeOrderReader(async () =>
      new Response('{}', { status: 200, headers: { 'content-type': 'text/plain' } }));
    await expect(invalid.read('co-1', 'order-1'))
      .rejects.toBeInstanceOf(NativeCommerceEmployeeOrderError);
  });

  it('rejects unbounded scope before any request', async () => {
    const transport: NativeCommerceOrdersTransport = vi.fn(async () => response({ data: order }));
    const reader = nativeCommerceEmployeeOrderReader(transport);
    await expect(reader.read('bad company!', 'order-1'))
      .rejects.toBeInstanceOf(NativeCommerceEmployeeOrderError);
    await expect(reader.read('co-1', 'bad id!'))
      .rejects.toBeInstanceOf(NativeCommerceEmployeeOrderError);
    expect(vi.mocked(transport)).not.toHaveBeenCalled();
  });
});
