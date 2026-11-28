import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativeCommerceReturnLineReader } from '../../../modules/commerce/ui/src/public/native-commerce-return-lines';
import { NativeCommerceOrdersError, type NativeCommerceOrdersTransport } from '../../../modules/commerce/ui/src/public/native-commerce-orders';

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44993',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
});
const incoming = () => new Headers({ host: 'acme.example.com' });
const line = {
  id: 'fl-1', fulfillment_id: 'f-1', order_line_id: 'ol-1',
  quantity_milli: 2000, value_minor: 500, subtotal_minor: 500, tax_minor: 35,
  product_id: 'p-1', item_type: 'stock', description: 'part', sku: 'SKU-1',
  returned_milli: 1000,
};
const response = (body: unknown) => new Response(JSON.stringify(body), {
  status: 200,
  headers: { 'content-type': 'application/json' },
});

describe('native Commerce return-line transport', () => {
  it('accepts the complete native payload', async () => {
    const transport: NativeCommerceOrdersTransport = vi.fn(async () => response({ data: [line] }));
    const reader = nativeCommerceReturnLineReader(config, transport);
    await expect(reader.read(incoming(), 'acme', 'company', 'order-1'))
      .resolves.toEqual([line]);
    const [, url, headers] = vi.mocked(transport).mock.calls[0]!;
    expect(url.pathname).toBe('/api/v1/holdings/acme/commerce/return-lines');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-record')).toBe('order-1');
  });

  it('fails closed on a partial or renamed payload', async () => {
    const partial = { ...line };
    delete (partial as Record<string, unknown>).returned_milli;
    const transport: NativeCommerceOrdersTransport = vi.fn(async () => response({ data: [partial] }));
    const reader = nativeCommerceReturnLineReader(config, transport);
    await expect(reader.read(incoming(), 'acme', 'company', 'order-1'))
      .rejects.toBeInstanceOf(NativeCommerceOrdersError);
  });

  it('never falls back on an unknown Holding or non-JSON reply', async () => {
    const transport: NativeCommerceOrdersTransport = vi.fn(async () => response({ data: [] }));
    const reader = nativeCommerceReturnLineReader(config, transport);
    await expect(reader.read(new Headers({ host: 'attacker.example.com' }), 'acme', 'company', 'order-1'))
      .rejects.toBeInstanceOf(NativeCommerceOrdersError);
    expect(vi.mocked(transport)).not.toHaveBeenCalled();
    const bad = nativeCommerceReturnLineReader(config, async () =>
      new Response('{}', { status: 200, headers: { 'content-type': 'text/plain' } }));
    await expect(bad.read(incoming(), 'acme', 'company', 'order-1'))
      .rejects.toBeInstanceOf(NativeCommerceOrdersError);
  });
});
