import { createHash } from 'node:crypto';
import { createServer, type Server } from 'node:http';
import { readFileSync } from 'node:fs';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  nativeCommerceEntrypointConfigured,
  nativeCommerceEntrypointFacts,
  nativeCommerceEntrypointKind,
  nativeCommerceInvoiceConfigured,
  nativeCommerceInvoiceSalesLines,
  nativeCommerceInvoiceTarget,
} from '../../../packages/server-commerce/src/adapters/native-lane-13/client';

const host = 'acme.example.test';
const origin = `https://${host}`;
const config = (privateBaseUrl: string) =>
  JSON.stringify({ privateBaseUrl, host, holdingSlug: 'acme', origin, timeoutMs: 1000 });

let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  delete process.env.ERPX_NATIVE_COMMERCE_LANE13_CONFIG;
  delete process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
  delete process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG;
});

type Request = { headers: Record<string, unknown>; url?: string; body: string };
async function listen(handler: (request: Request) => { status: number; value: unknown }): Promise<string> {
  server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on('data', chunk => chunks.push(Buffer.from(chunk)));
    req.on('end', () => {
      const result = handler({
        headers: req.headers,
        url: req.url,
        body: Buffer.concat(chunks).toString('utf8'),
      });
      res.writeHead(result.status, { 'content-type': 'application/json' });
      res.end(JSON.stringify(result.value));
    });
  });
  await new Promise<void>(resolve => server!.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no listener');
  return `http://127.0.0.1:${address.port}`;
}

const sha = (value: string) => createHash('sha256').update(value, 'utf8').digest('hex');

/** Rendered SQL/result oracle mirrored from the owner crate. */
const SQL = [
  'SELECT kind FROM commerce_orders WHERE id=$1 AND company_id=$2',
  'SELECT kind,state FROM commerce_orders WHERE id=$1 AND company_id=$2',
  'SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=$1 AND o.company_id=$2',
  'SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.invoice_id=$1 AND o.company_id=$2',
  'SELECT o.kind FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id WHERE r.id=$1 AND o.company_id=$2',
];

describe('native Commerce entrypoint read contract', () => {
  it('pins the rendered SQL and result oracles', () => {
    expect(sha(SQL.join('\n'))).toBe('b1dd9830f8cf1f007e2119df70f21ed6f53c7baa61d1b6ff15ad362376ee39b1');
    expect(sha(JSON.stringify({ facts: { kind: 'sales', state: 'draft' } }))).toBe(
      '308e156923b8a994c1c30414eb4043579d3739b63ddfbd9e2abdb0578d92a508',
    );
    expect(sha(JSON.stringify({ facts: { kind: 'sales' } }))).toBe(
      '49435cfa0febf78526788d491b4ed66804c9809ea6cd25e105fbece3bc0e982e',
    );
  });

  it('fails closed when the native transport is not configured', async () => {
    expect(nativeCommerceEntrypointConfigured()).toBe(false);
    await expect(nativeCommerceEntrypointKind('orderKind', 'o-1', 'co-1')).rejects.toMatchObject({
      code: 'COMMERCE_ENTRYPOINT_READ_CONFIG_REQUIRED',
      status: 503,
    });
  });

  it('fails closed on a malformed native configuration', async () => {
    process.env.ERPX_NATIVE_COMMERCE_LANE13_CONFIG = config('http://127.0.0.1:3100');
    await expect(nativeCommerceEntrypointFacts('o-1', 'co-1')).rejects.toMatchObject({
      code: 'COMMERCE_ENTRYPOINT_READ_CONFIG_INVALID',
      status: 503,
    });
  });

  it('sends only the discriminator and bounded id, then pins the response shape', async () => {
    let request: Request | undefined;
    const base = await listen(req => {
      request = req;
      return { status: 200, value: { facts: { kind: 'sales', state: 'draft' } } };
    });
    process.env.ERPX_NATIVE_COMMERCE_LANE13_CONFIG = config(base);
    await expect(nativeCommerceEntrypointFacts('o-1', 'co-1')).resolves.toEqual({ kind: 'sales', state: 'draft' });
    expect(JSON.parse(request!.body)).toEqual({ kind: 'orderFacts', id: 'o-1' });
    expect(request!.headers).toMatchObject({
      host,
      origin,
      accept: 'application/json',
      'content-type': 'application/json',
      'x-erpx-context-company': 'co-1',
    });
    expect(request!.url).toBe('/api/v1/holdings/acme/commerce/entrypoint-facts');
    expect(request!.body).not.toContain('SELECT ');
  });

  it('returns null for an absent record and pins the kind-only reply', async () => {
    const base = await listen(() => ({ status: 200, value: { facts: null } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_CONFIG = config(base);
    await expect(nativeCommerceEntrypointKind('returnKind', 'missing', 'co-1')).resolves.toBeNull();

    server?.close();
    const second = await listen(() => ({ status: 200, value: { facts: { kind: 'purchase' } } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_CONFIG = config(second);
    await expect(nativeCommerceEntrypointKind('invoiceKind', 'f-1', 'co-1')).resolves.toEqual({ kind: 'purchase' });
  });

  it('fails closed on malformed replies and upstream failures', async () => {
    let base = await listen(() => ({ status: 200, value: { data: {} } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_CONFIG = config(base);
    await expect(nativeCommerceEntrypointKind('orderKind', 'o-1', 'co-1')).rejects.toMatchObject({
      code: 'COMMERCE_ENTRYPOINT_READ_RESPONSE_INVALID',
      status: 503,
    });

    server?.close();
    base = await listen(() => ({ status: 200, value: { facts: { kind: 'sales', state: 'draft' } } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_CONFIG = config(base);
    await expect(nativeCommerceEntrypointKind('orderKind', 'o-1', 'co-1')).rejects.toMatchObject({
      code: 'COMMERCE_ENTRYPOINT_READ_RESPONSE_INVALID',
      status: 503,
    });

    server?.close();
    base = await listen(() => ({ status: 503, value: { error: { code: 'DOWN', message: 'down' } } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_CONFIG = config(base);
    await expect(nativeCommerceEntrypointFacts('o-1', 'co-1')).rejects.toMatchObject({ status: 503 });
  });

  it('retired executable TypeScript SQL; retained only the shared projection constants', () => {
    const source = readFileSync(
      new URL('../../../packages/server-commerce/src/adapters/entrypoint-facts.ts', import.meta.url),
      'utf8',
    );
    expect((source.match(/SELECT /g) ?? []).length).toBe(3);
    expect(source).toContain("settings:'SELECT company_id,chart_id FROM commerce_settings'");
    expect(source).toContain("warehouses:'SELECT id,branch_id FROM commerce_warehouses'");
    expect(source).toContain("products:'SELECT id,sku,name FROM commerce_products'");
    for (const retired of [
      'SELECT kind FROM commerce_orders',
      'SELECT kind,state FROM commerce_orders',
      'SELECT o.kind FROM commerce_fulfillments',
      'SELECT o.kind FROM commerce_returns',
      'commerceFulfillmentKindSql',
      '.prepare(',
    ])
      expect(source).not.toContain(retired);
    expect(source).toContain('nativeCommerceEntrypointKind');
    expect(source).toContain('nativeCommerceEntrypointFacts');
  });
});

/** Rendered invoice/delivery SQL mirrored from the owner crate. */
const INVOICE_SQL = [
  'SELECT f.order_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=$1 AND o.company_id=$2',
  "SELECT fl.id,fl.fulfillment_id,ol.description,fl.quantity_milli,ol.price_minor,ol.tax_bps,fl.subtotal_minor,fl.tax_minor,ol.item_type,ol.product_id,f.fulfillment_date completion_date FROM commerce_fulfillment_lines fl JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_orders o ON o.id=f.order_id WHERE f.invoice_id=$1 AND o.company_id=$2 AND o.kind='sales' ORDER BY f.created_at,f.id,ol.ordinal,fl.id",
];

const holdingsConfig = (privateBaseUrl: string) =>
  JSON.stringify({
    privateBaseUrl,
    timeoutMs: 1000,
    defaultHoldingSlug: 'acme',
    holdings: [
      { origin, host, holdingId: 'h-acme', holdingSlug: 'acme' },
      {
        origin: 'https://northstar.example.test',
        host: 'northstar.example.test',
        holdingId: 'h-northstar',
        holdingSlug: 'northstar',
      },
    ],
  });

const line = {
  id: 'fl-1',
  fulfillment_id: 'f-1',
  description: 'service',
  quantity_milli: 1000,
  price_minor: 250,
  tax_bps: 700,
  subtotal_minor: 250,
  tax_minor: 18,
  item_type: 'service',
  product_id: 'p-1',
  completion_date: '2026-11-25',
};

describe('native Commerce invoice/delivery contract (multi-holding)', () => {
  it('pins the invoice SQL and disjoint response oracles', () => {
    expect(sha(INVOICE_SQL.join('\n'))).toBe(
      'a0b282e0aa679a4ec0ffb3c02242f6eb24615795195aeab352b7909133bf72bd',
    );
    expect(sha(JSON.stringify({ target: { kind: 'order', id: 'o-1', action: 'invoice' } }))).toBe(
      '076efb877692b81cf30e95c7750bdc12283178c1ba9bd95e69b346e99d8f15ce',
    );
    expect(sha(JSON.stringify({ target: null }))).toBe(
      '0986740ebf1eb9d4cb6080cb99257995a35a0894ad10a4913af35176c9e72bff',
    );
  });

  it('fails closed when the multi-holding transport is absent or malformed', async () => {
    expect(nativeCommerceInvoiceConfigured()).toBe(false);
    await expect(nativeCommerceInvoiceTarget('d-1', { companyId: 'co-1' })).rejects.toMatchObject({
      code: 'COMMERCE_INVOICE_READ_CONFIG_REQUIRED',
      status: 503,
    });
    process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG = holdingsConfig('http://127.0.0.1:3100');
    await expect(nativeCommerceInvoiceTarget('d-1', { companyId: 'co-1' })).rejects.toMatchObject({
      code: 'COMMERCE_INVOICE_READ_CONFIG_INVALID',
      status: 503,
    });
    const bad = JSON.parse(holdingsConfig('http://127.0.0.1:41234'));
    bad.holdings[1].host = host;
    process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG = JSON.stringify(bad);
    await expect(nativeCommerceInvoiceSalesLines('inv-1', { companyId: 'co-1' })).rejects.toMatchObject({
      code: 'COMMERCE_INVOICE_READ_CONFIG_INVALID',
      status: 503,
    });
  });

  it('selects the exact actor Holding and pins the target wire/shape', async () => {
    let request: Request | undefined;
    const base = await listen(req => {
      request = req;
      return { status: 200, value: { target: { kind: 'order', id: 'o-1', action: 'invoice' } } };
    });
    process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG = holdingsConfig(base);
    await expect(
      nativeCommerceInvoiceTarget('d-1', {
        companyId: 'co-1',
        holdingId: 'h-northstar',
        holdingSlug: 'northstar',
      }),
    ).resolves.toEqual({ kind: 'order', id: 'o-1', action: 'invoice' });
    expect(request!.headers).toMatchObject({ host: 'northstar.example.test', 'x-erpx-context-company': 'co-1' });
    expect(request!.url).toBe('/api/v1/holdings/northstar/commerce/invoice-read');
    expect(JSON.parse(request!.body)).toEqual({ kind: 'deliveryTarget', id: 'd-1' });
    expect(request!.body).not.toContain('SELECT ');
  });

  it('uses the default Holding and returns null for an absent target', async () => {
    let request: Request | undefined;
    const base = await listen(req => {
      request = req;
      return { status: 200, value: { target: null } };
    });
    process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG = holdingsConfig(base);
    await expect(nativeCommerceInvoiceTarget('d-1', { companyId: 'co-1' })).resolves.toBeNull();
    expect(request!.headers).toMatchObject({ host });
    expect(request!.url).toBe('/api/v1/holdings/acme/commerce/invoice-read');
  });

  it('rejects a changed Holding context with 409', async () => {
    const base = await listen(() => ({ status: 200, value: { target: null } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG = holdingsConfig(base);
    await expect(
      nativeCommerceInvoiceTarget('d-1', {
        companyId: 'co-1',
        holdingId: 'h-unknown',
        holdingSlug: 'acme',
      }),
    ).rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
  });

  it('validates the sales-line envelope strictly', async () => {
    const base = await listen(() => ({ status: 200, value: { lines: [line] } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG = holdingsConfig(base);
    await expect(nativeCommerceInvoiceSalesLines('inv-1', { companyId: 'co-1' })).resolves.toEqual([line]);

    server?.close();
    const malformed = await listen(() => ({ status: 200, value: { lines: [{ ...line, tax_bps: 'x' }] } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG = holdingsConfig(malformed);
    await expect(nativeCommerceInvoiceSalesLines('inv-1', { companyId: 'co-1' })).rejects.toMatchObject({
      code: 'COMMERCE_INVOICE_READ_RESPONSE_INVALID',
      status: 503,
    });

    server?.close();
    const confused = await listen(() => ({ status: 200, value: { target: null } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG = holdingsConfig(confused);
    await expect(nativeCommerceInvoiceSalesLines('inv-1', { companyId: 'co-1' })).rejects.toMatchObject({
      code: 'COMMERCE_INVOICE_READ_RESPONSE_INVALID',
      status: 503,
    });
  });

  it('fails closed on an upstream failure', async () => {
    const base = await listen(() => ({ status: 503, value: { error: { code: 'DOWN' } } }));
    process.env.ERPX_NATIVE_COMMERCE_LANE13_HOLDINGS_CONFIG = holdingsConfig(base);
    await expect(nativeCommerceInvoiceTarget('d-1', { companyId: 'co-1' })).rejects.toMatchObject({
      code: 'COMMERCE_INVOICE_READ_UNAVAILABLE',
      status: 503,
    });
  });

  it('deleted the executable TypeScript SQL for both retired invoice reads', () => {
    const adapters = (file: string) =>
      new URL(`../../../packages/server-commerce/src/adapters/${file}`, import.meta.url);
    const target = readFileSync(adapters('invoice-command-policy.ts'), 'utf8');
    expect((target.match(/SELECT /g) ?? []).length).toBe(0);
    expect(target).toContain('nativeCommerceInvoiceTarget');
    expect(target).not.toContain('commerce_fulfillments f');
    const sales = readFileSync(adapters('finance-sales-sources.ts'), 'utf8');
    expect((sales.match(/SELECT /g) ?? []).length).toBe(1);
    expect(sales).toContain('invoiceScope: "SELECT f.invoice_id');
    expect(sales).toContain('nativeCommerceInvoiceSalesLines');
    expect(sales).not.toContain('fl.quantity_milli');
  });
});
