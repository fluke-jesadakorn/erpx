import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const read = (path: string) => readFileSync(resolve(__dirname, '../../..', path), 'utf8');

describe('native POS workspace customer search', () => {
  it('routes the workspace customer read through the native owner', () => {
    const source = read('packages/server-pos/src/application/pos-daily.ts');
    expect(source).toContain('nativePosCustomerSearch(actor, query.trim().slice(0, 100))');
    expect(source).not.toContain('db.readFinanceParties2');
  });

  it('uses the Host-preserving native transport, never fetch', () => {
    const client = read('packages/server-pos/src/adapters/native_customer_search/client.ts');
    expect(client).toContain('nativeRequest(url, { method: "GET"');
    expect(client).not.toContain('await fetch(');
    expect(client).not.toContain('SELECT ');
  });
});
