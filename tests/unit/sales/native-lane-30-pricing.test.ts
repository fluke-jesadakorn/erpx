import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  NATIVE_SALES_PRICE_LIST_CONFIG_ENV,
  createPriceListWorkspaceClient,
  readPriceListWorkspace,
  type PriceListActor,
} from '../../../packages/server-sales/src/adapters/native-lane-30/pricing-client';
import { ApiError } from '@erpx/server-runtime/public/http';

const config = {
  privateBaseUrl: 'http://127.0.0.1:44998',
  timeoutMs: 1000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co' },
    { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar', companyId: 'northstar-co' },
    { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'erpx-co' },
  ],
};
let raw = JSON.stringify(config);
const actorFor = (holdingId: string, holdingSlug: string, companyId: string): PriceListActor => ({
  holdingId, holdingSlug, companyId,
});

const list = {
  id: 'list-1', company_id: 'acme-co', name: 'Standard', currency: 'THB',
  customer_id: null, valid_from: '2026-01-01', valid_until: '2026-12-31',
  max_auto_discount_bps: 0, status: 'draft', version: 1, created_by: 'user-1',
  approved_by: null, approval_reason: null, activated_at: null,
  created_at: 10, updated_at: 11, customer_name: null,
};
const item = {
  id: 'item-1', price_list_id: 'list-1', product_id: 'product-1',
  price_minor: 100, tax_bps: 700, ordinal: 1, sku: 'SKU-1', product_name: 'Product',
};
const payloadFor = (company: string) => ({ lists: [{ ...list, company_id: company }], items: [item] });
const payload = payloadFor('acme-co');

type Seen = Readonly<{
  url: string;
  host: string | undefined;
  origin: string | undefined;
  company: string | undefined;
}>;
let seen: Seen;
let reply: { status: number; contentType: string; body: unknown; delayMs: number };
const server = createServer((request, result) => {
  result.on('error', () => {});
  seen = {
    url: request.url ?? '',
    host: request.headers.host,
    origin: request.headers.origin,
    company: request.headers['x-erpx-context-company'] as string | undefined,
  };
  const company = String(request.headers['x-erpx-context-company'] ?? '');
  const send = () => {
    result.writeHead(reply.status, { 'content-type': reply.contentType });
    result.end(typeof reply.body === 'string' ? reply.body : JSON.stringify(reply.body ?? payloadFor(company)));
  };
  if (reply.delayMs > 0) setTimeout(send, reply.delayMs);
  else send();
});
beforeAll(async () => {
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Expected a loopback listener');
  config.privateBaseUrl = `http://127.0.0.1:${address.port}`;
  raw = JSON.stringify(config);
});
beforeEach(() => { reply = { status: 200, contentType: 'application/json', body: undefined, delayMs: 0 }; });
afterAll(() => new Promise<void>((resolve, reject) => {
  server.close(error => error ? reject(error) : resolve());
}));

const json = (value: unknown) =>
  new Response(JSON.stringify(value), { status: 200, headers: { 'content-type': 'application/json' } });

async function expect503(call: () => Promise<unknown>, code?: string) {
  try {
    await call();
    throw new Error('expected a fail-closed ApiError');
  } catch (error) {
    expect(error).toBeInstanceOf(ApiError);
    const api = error as ApiError;
    expect(api.status).toBe(503);
    if (code) expect(api.code).toBe(code);
  }
}

describe('native lane 30 price-list workspace client', () => {
  it('pins the response-shape oracle hash', () => {
    expect(createHash('sha256').update(JSON.stringify(payload)).digest('hex')).toBe(
      '75367e6645aea0e4163f219925d8f280d4106b155696d41a617c97ecedb39b4c',
    );
    expect(Object.keys(payload)).toEqual(['lists', 'items']);
    expect(Object.keys(list)).toHaveLength(17);
    expect(Object.keys(item)).toHaveLength(8);
  });

  it('preserves the registered Holding Host on the default loopback transport', async () => {
    const client = createPriceListWorkspaceClient(raw);
    expect(await client.read(actorFor('h001', 'acme', 'acme-co'))).toEqual(payload);
    expect(seen).toEqual({
      url: '/api/v1/holdings/acme/sales/price-list-workspace',
      host: 'acme.jesadakorn.com',
      origin: 'https://acme.jesadakorn.com',
      company: 'acme-co',
    });
    expect(seen.host).not.toContain('127.0.0.1');
    const northstar = await client.read(actorFor('h002', 'northstar', 'northstar-co'));
    expect(northstar.lists[0].company_id).toBe('northstar-co');
    expect(seen).toEqual({
      url: '/api/v1/holdings/northstar/sales/price-list-workspace',
      host: 'northstar.jesadakorn.com',
      origin: 'https://northstar.jesadakorn.com',
      company: 'northstar-co',
    });
  });

  it('keeps status, content-type, timeout and byte bounds on the default transport', async () => {
    const client = createPriceListWorkspaceClient(raw);
    reply = { status: 503, contentType: 'application/json', body: {}, delayMs: 0 };
    await expect503(() => client.read(actorFor('h001', 'acme', 'acme-co')));
    reply = { status: 200, contentType: 'text/plain', body: payload, delayMs: 0 };
    await expect503(
      () => client.read(actorFor('h001', 'acme', 'acme-co')),
      'SALES_NATIVE_PRICE_LIST_RESPONSE_INVALID',
    );
    reply = { status: 200, contentType: 'application/json', body: 'x'.repeat(2 * 1024 * 1024 + 1), delayMs: 0 };
    await expect503(() => client.read(actorFor('h001', 'acme', 'acme-co')));
    reply = { status: 200, contentType: 'application/json', body: payload, delayMs: 500 };
    const slow = createPriceListWorkspaceClient(JSON.stringify({ ...config, timeoutMs: 250 }));
    await expect503(() => slow.read(actorFor('h001', 'acme', 'acme-co')));
  });

  it('selects the actor Holding for all three registered targets, never the first', async () => {
    const calls: { host: string; path: string; company: string }[] = [];
    const client = createPriceListWorkspaceClient(raw, async (url, headers) => {
      const company = headers.get('x-erpx-context-company')!;
      calls.push({ host: headers.get('host')!, path: url.pathname, company });
      return json(payloadFor(company));
    });
    const northstar = await client.read(actorFor('h002', 'northstar', 'northstar-co'));
    const erpx = await client.read(actorFor('h003', 'erpx', 'erpx-co'));
    const acme = await client.read(actorFor('h001', 'acme', 'acme-co'));
    expect(calls).toEqual([
      { host: 'northstar.jesadakorn.com', path: '/api/v1/holdings/northstar/sales/price-list-workspace', company: 'northstar-co' },
      { host: 'erpx-internal.jesadakorn.com', path: '/api/v1/holdings/erpx/sales/price-list-workspace', company: 'erpx-co' },
      { host: 'acme.jesadakorn.com', path: '/api/v1/holdings/acme/sales/price-list-workspace', company: 'acme-co' },
    ]);
    expect(northstar.lists[0].company_id).toBe('northstar-co');
    expect(erpx.lists[0].company_id).toBe('erpx-co');
    expect(acme.lists[0].company_id).toBe('acme-co');
  });

  it('fails closed for unknown target and company pin mismatch with no owner call', async () => {
    let calls = 0;
    const client = createPriceListWorkspaceClient(raw, async () => {
      calls += 1;
      return json({ lists: [], items: [] });
    });
    await expect503(() => client.read(actorFor('h999', 'ghost', 'ghost-co')), 'SALES_NATIVE_PRICE_LIST_TARGET_INVALID');
    await expect503(() => client.read(actorFor('h002', 'northstar', 'acme-co')), 'SALES_NATIVE_PRICE_LIST_TARGET_INVALID');
    expect(calls).toBe(0);
  });

  it('rejects malformed and cross-company responses strictly', async () => {
    const bad = [
      { ...payload, extra: 1 },
      { lists: [list], items: {} },
      { lists: [{ ...list, company_id: 'other' }], items: [] },
      { lists: [{ ...list, unexpected: true }], items: [] },
      { lists: [], items: [{ ...item, version: 1 }] },
      { lists: [{ ...list, id: 'list-1' }, { ...list, id: 'list-1' }], items: [] },
    ];
    for (const value of bad) {
      const client = createPriceListWorkspaceClient(raw, async () => json(value));
      await expect503(
        () => client.read(actorFor('h001', 'acme', 'acme-co')),
        'SALES_NATIVE_PRICE_LIST_RESPONSE_INVALID',
      );
    }
  });

  it('fails closed on non-200, wrong content type, transport error and bad config', async () => {
    const non200 = createPriceListWorkspaceClient(raw, async () => new Response('{}', { status: 503 }));
    await expect503(() => non200.read(actorFor('h001', 'acme', 'acme-co')));
    const wrongType = createPriceListWorkspaceClient(
      raw,
      async () => new Response(JSON.stringify(payload), { status: 200, headers: { 'content-type': 'text/plain' } }),
    );
    await expect503(
      () => wrongType.read(actorFor('h001', 'acme', 'acme-co')),
      'SALES_NATIVE_PRICE_LIST_RESPONSE_INVALID',
    );
    const offline = createPriceListWorkspaceClient(raw, async () => { throw new Error('offline'); });
    await expect503(() => offline.read(actorFor('h001', 'acme', 'acme-co')));
    expect(() => createPriceListWorkspaceClient('not json')).toThrow(ApiError);
    expect(() => createPriceListWorkspaceClient(JSON.stringify({ ...config, holdings: [] }))).toThrow(ApiError);
    expect(() => createPriceListWorkspaceClient(
      JSON.stringify({ ...config, holdings: [{ ...config.holdings[0], host: 'evil.example.com' }] }),
    )).toThrow(ApiError);
  });

  it('fails closed through the environment-backed entry point when unconfigured', async () => {
    delete process.env[NATIVE_SALES_PRICE_LIST_CONFIG_ENV];
    await expect503(
      () => readPriceListWorkspace(actorFor('h001', 'acme', 'acme-co')),
      'SALES_NATIVE_PRICE_LIST_CONFIG_REQUIRED',
    );
    process.env[NATIVE_SALES_PRICE_LIST_CONFIG_ENV] = raw;
    try {
      const result = await readPriceListWorkspace(actorFor('h001', 'acme', 'acme-co'), async () => json(payload));
      expect(result).toEqual(payload);
    } finally {
      delete process.env[NATIVE_SALES_PRICE_LIST_CONFIG_ENV];
    }
  });
});
