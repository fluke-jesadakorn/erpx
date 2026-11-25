import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const adapters = resolve(root, 'packages/server-commerce/src/adapters');
const src = resolve(root, 'packages/server-commerce/src');
const read = (file: string) => readFileSync(resolve(adapters, file), 'utf8');

const count = (dir: string, pattern: RegExp): number => {
  let total = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = resolve(dir, entry.name);
    if (entry.isDirectory()) total += count(path, pattern);
    else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(pattern) ?? []).length;
  }
  return total;
};

/** r29 family: fulfillment/invoice order-kind projection. The two TS call
 * sites are `fulfillmentKind` / `invoiceKind` in entrypoint-facts.ts. Both now
 * delegate to the mounted lane-13 axum/sqlx owner port; the TS renderer that
 * used to prepare the shared head is deleted by this cutover. */
describe('Commerce r29 fulfillment/invoice kind family native cutover', () => {
  test('all five TS entrypoint kind readers delegate to the native owner port', () => {
    const text = read('entrypoint-facts.ts');
    expect(text).toContain(
      "import {nativeCommerceEntrypointFacts,nativeCommerceEntrypointKind} from './native-lane-13/client'",
    );
    for (const kind of ['orderKind', 'fulfillmentKind', 'invoiceKind', 'returnKind'])
      expect(text).toContain(`${kind}:(id:string,companyId:string)=>nativeCommerceEntrypointKind('${kind}',id,companyId)`);
    expect(text).toContain('orderFacts:(id:string,companyId:string)=>nativeCommerceEntrypointFacts(id,companyId)');
    expect(text).not.toContain('.prepare(');
    expect(text).not.toContain('SELECT o.kind FROM commerce_fulfillments');
    expect(text).not.toContain('SELECT o.kind FROM commerce_returns');
    // The three exported cross-owner projection constants are still live.
    expect((text.match(/SELECT /g) ?? []).length).toBe(3);
  });

  test('replaced TS SQL is deleted and the exact literals live in the native plans', () => {
    expect(existsSync(resolve(adapters, 'read-tail-r29-dedupe-sql.ts'))).toBe(false);
    expect(count(src, /SELECT o\.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o\.id=f\.order_id WHERE /g)).toBe(0);
    expect(count(src, /SELECT o\.kind FROM commerce_returns r JOIN commerce_orders o ON o\.id=r\.order_id /g)).toBe(0);
    const plans = read('native_lane_13/postgres.rs');
    expect(plans).toContain(
      'JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=$1 AND o.company_id=$2',
    );
    expect(plans).toContain(
      'JOIN commerce_orders o ON o.id=f.order_id WHERE f.invoice_id=$1 AND o.company_id=$2',
    );
    expect(plans).toContain('EntrypointKind::FulfillmentKind => FULFILLMENT_KIND_SQL');
    expect(plans).toContain('EntrypointKind::InvoiceKind => INVOICE_KIND_SQL');
    const client = read('native-lane-13/client.ts');
    expect(client).toContain("export type EntrypointKind = 'orderKind' | 'orderFacts' | 'fulfillmentKind' | 'invoiceKind' | 'returnKind'");
    expect(client).toContain("nativeCommerceEntrypointKind(kind: Exclude<EntrypointKind, 'orderFacts'>");
    expect(client).toContain('/commerce/entrypoint-facts');
    expect(client).not.toContain('SELECT ');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 364 (before 367)', () => {
    expect(count(src, /SELECT /g)).toBe(364);
    expect((read('entrypoint-facts.ts').match(/SELECT /g) ?? []).length).toBe(3);
  });
});
