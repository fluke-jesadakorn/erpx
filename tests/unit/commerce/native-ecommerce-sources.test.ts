import { createServer, type Server } from 'node:http';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  ecommerceCurrencyConfiguration,
  NativeCommerceEcommerceSourcesError,
} from '../../../packages/server-commerce/src/adapters/native-ecommerce-sources';

const host = 'acme.example.com';
const origin = `https://${host}`;
const company = 'company';
const database = {} as Parameters<typeof ecommerceCurrencyConfiguration>[0];

const config = (privateBaseUrl: string) =>
  JSON.stringify({ privateBaseUrl, host, holdingSlug: 'acme', origin, timeoutMs: 1000 });

let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  delete process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
});

async function listen(
  handler: (req: { headers: Record<string, unknown>; url?: string }) => {
    status: number;
    value: unknown;
  },
): Promise<string> {
  server = createServer((req, res) => {
    const result = handler(req);
    res.writeHead(result.status, { 'content-type': 'application/json' });
    res.end(JSON.stringify(result.value));
  });
  await new Promise<void>(resolve => server!.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no listener');
  return `http://127.0.0.1:${address.port}`;
}

describe('native Ecommerce configuration read transport', () => {
  it('fails closed when the native owner transport is not configured', async () => {
    delete process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
    await expect(ecommerceCurrencyConfiguration(database, company)).rejects.toMatchObject({
      code: 'ECOMMERCE_CONFIGURATION_READ_UNAVAILABLE',
      status: 503,
    });
  });

  it('fails closed on a malformed native owner transport configuration', async () => {
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config('http://127.0.0.1:3100');
    await expect(ecommerceCurrencyConfiguration(database, company)).rejects.toBeInstanceOf(
      NativeCommerceEcommerceSourcesError,
    );
  });

  it('carries no SQL and maps the mounted currency route configuration reply', async () => {
    const seen: Record<string, unknown>[] = [];
    const base = await listen(req => {
      seen.push(req.headers);
      return {
        status: 200,
        value: { configuration: { currency: 'THB', base_currency: 'USD' } },
      };
    });
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config(base);
    await expect(ecommerceCurrencyConfiguration(database, company)).resolves.toEqual({
      currency: 'THB',
      base_currency: 'USD',
    });
    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatchObject({
      host,
      origin,
      accept: 'application/json',
      'x-erpx-context-company': company,
      'x-erpx-ecommerce-configuration': '1',
    });
  });

  it('returns null for an unconfigured company', async () => {
    const base = await listen(() => ({ status: 200, value: { configuration: null } }));
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config(base);
    await expect(ecommerceCurrencyConfiguration(database, company)).resolves.toBeNull();
  });

  it('fails closed on a malformed native reply', async () => {
    const base = await listen(() => ({
      status: 200,
      value: { configuration: { currency: 'thb', base_currency: 'USD' } },
    }));
    process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG = config(base);
    await expect(ecommerceCurrencyConfiguration(database, company)).rejects.toMatchObject({
      code: 'ECOMMERCE_CONFIGURATION_READ_RESPONSE_INVALID',
      status: 503,
    });
  });
});
