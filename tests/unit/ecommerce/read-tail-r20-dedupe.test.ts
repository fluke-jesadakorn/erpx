import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { ecommerceExistsGuard, ecommerceScopeRow } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const po = read('adapters/package-obligations.ts');
const ar = read('adapters/attachment-read.ts');
const pb = read('adapters/purchase-boundary.ts');
const shared = read('adapters/read-tail-sql.ts');
const ecommerce = read('adapters/ecommerce.ts');

const deliveries = '<deliveries>';

// Pre-dedupe SQL copied verbatim from HEAD (r19), kept only as byte-identity oracles.
const head = {
  moneySnapshot: `NOT EXISTS(SELECT 1 FROM money_transaction_snapshots captured WHERE captured.company_id=channel.company_id AND captured.source_type='ecommerce_listing' AND captured.source_id=listing.id AND captured.amount_kind='unit_price' AND captured.currency=listing.currency AND captured.amount_units=listing.price_minor)`,
  checkoutDelivery: `EXISTS(SELECT 1 FROM (${deliveries}) delivery WHERE delivery.order_id=checkout.sales_order_id AND NOT EXISTS(SELECT 1 FROM ecommerce_fulfillment_links link WHERE link.checkout_id=checkout.id AND link.fulfillment_id=delivery.id))`,
  workforceRow: `(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope`,
  workforceProbe: `SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${'${current.where}'}`,
  receiptProbe: `SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?`,
};

// Frames rebuilt exactly as the adapters define them (r14 exists, r20 separator).
const moneySnapshot = `NOT ${ecommerceExistsGuard('money_transaction_snapshots captured', "captured.company_id=channel.company_id AND captured.source_type='ecommerce_listing' AND captured.source_id=listing.id AND captured.amount_kind='unit_price' AND captured.currency=listing.currency AND captured.amount_units=listing.price_minor")}`;
const fulfillmentLink = ecommerceExistsGuard('ecommerce_fulfillment_links link', 'link.checkout_id=checkout.id AND link.fulfillment_id=delivery.id');
const checkoutDelivery = ecommerceExistsGuard(`(${deliveries}) delivery`, `delivery.order_id=checkout.sales_order_id AND NOT ${fulfillmentLink}`);
const workforceRow = ecommerceScopeRow('workforce_actor_scope', true, '');
const workforceProbe = `SELECT 1 FROM${ecommerceScopeRow('workforce_actor_scope', true, '')} WHERE ${'${current.where}'}`;

describe('Ecommerce read-tail r20 C5-C9 dedupe', () => {
  test('money-transaction snapshot NOT EXISTS rebuilds byte-identically from the r14 frame', () => {
    expect(moneySnapshot).toBe(head.moneySnapshot);
    expect(fulfillmentLink).toBe('EXISTS(SELECT 1 FROM ecommerce_fulfillment_links link WHERE link.checkout_id=checkout.id AND link.fulfillment_id=delivery.id)');
  });

  test('fulfillment delivery and link guards rebuild byte-identically from the r14 frame', () => {
    expect(checkoutDelivery).toBe(head.checkoutDelivery);
  });

  test('r20 separator rebuilds the 3-column workforce row and its probe byte-identically', () => {
    expect(workforceRow).toBe(head.workforceRow);
    expect(workforceProbe).toBe(head.workforceProbe);
    // Existing default-separator callers are unchanged.
    expect(ecommerceScopeRow('ecommerce_file_review', true)).toBe('(SELECT ?::text company_id,?::text branch_id,?::text department_id) ecommerce_file_review');
    expect(ecommerceScopeRow('ecommerce_file')).toBe('(SELECT ?::text company_id) ecommerce_file');
  });

  test('raw literals are gone and each family composes once from the shared frame', () => {
    expect(po).not.toContain('NOT EXISTS(SELECT 1 FROM money_transaction_snapshots captured');
    expect(po).not.toContain('EXISTS(SELECT 1 FROM (${commerce.deliveries}) delivery');
    expect(ar).not.toContain(head.workforceRow);
    expect(po).toContain("import {ecommerceExistsGuard} from './read-tail-sql';");
    expect(ar).toContain("import {ecommerceListingEvidence,ecommerceScopeRow} from './read-tail-sql';");
    expect((po.match(/ecommerceExistsGuard\(/g) ?? []).length).toBe(3);
    expect((ar.match(/ecommerceScopeRow\(/g) ?? []).length).toBe(1);
    expect(shared.match(/export const ecommerceExistsGuard = /g) ?? []).toHaveLength(1);
    expect(shared.match(/export const ecommerceScopeRow = /g) ?? []).toHaveLength(1);
    expect(po).toContain("NOT ${ecommerceExistsGuard('money_transaction_snapshots captured'");
    expect(po).toContain('OR ${ecommerceExistsGuard(`(${commerce.deliveries}) delivery`');
    expect(po).toContain("NOT ${ecommerceExistsGuard('ecommerce_fulfillment_links link'");
    expect(ar).toContain("`SELECT 1 FROM${ecommerceScopeRow('workforce_actor_scope', true, '')} WHERE ${current.where}`");
  });

  test('binds and locks stay unchanged; composed guards stay read-only', () => {
    expect((head.moneySnapshot.match(/\?/g) ?? []).length).toBe(0);
    expect((head.checkoutDelivery.match(/\?/g) ?? []).length).toBe(0);
    expect((head.workforceProbe.match(/\?/g) ?? []).length).toBe(3);
    expect((head.receiptProbe.match(/\?/g) ?? []).length).toBe(5);
    expect((po.match(/\?/g) ?? []).length).toBe(1);
    for (const sql of Object.values(head)) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(po).toContain('.bind(input.companyId)');
    expect(ar).toContain('.bind(actor.companyId,actor.branchId,actor.departmentId,...current.args)');
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(po).toContain('salesCommerceSettlementSourceReadLocks(database)');
    expect(po).toContain('ecommerceCommerceSourceReadLocks(database)');
    expect(po).toContain('lockEcommerceFilePackageObligationSources(owner,lock)');
    expect(ar).toContain('ecommerceCommerceSourceReadLocks(database)');
  });

  test('receipt duplicate probe is left alone because it is not byte-identical to a frame', () => {
    expect(pb).toContain(head.receiptProbe);
    expect((pb.match(/SELECT 1 FROM command_receipts WHERE command_id=\?/g) ?? []).length).toBe(1);
    expect(po).not.toContain(head.receiptProbe);
    expect(ar).not.toContain(head.receiptProbe);
    expect(pb).not.toContain('ecommerceExistsGuard');
    expect(pb).not.toContain('receiptAuditGuard');
  });

  test('native, locks and r1-r19 frame sites stay intact', () => {
    expect(po).toContain('const receiptAuditGuard=');
    expect(po).toContain('const cartHasCheckout=');
    expect(po).toContain('${reviewReceiptGuard(\'listing\')}');
    expect(po).toContain('AND ${checkoutSyncGuard},false)');
    expect(po).toContain('OR ${checkoutReturnGuard} FROM ecommerce_return_links');
    expect((po.match(/\$\{receiptAuditGuard\(/g) ?? []).length).toBe(2);
    for (const token of ['ecommerceModuleAssignment', 'ecommerceExistsGuard', 'ecommerceOrderedList', 'ecommerceListingChannelRow', 'ecommerceSourceIdentity', 'ecommerceListingEvidence']) {
      expect(shared).toContain(`export const ${token} =`);
    }
    expect(read('adapters/attachment-purchases.ts')).toContain('LOCK TABLE commerce_products IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
    expect(ecommerce).toContain('SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE');
    expect(ecommerce).toContain('FOR UPDATE OF x,c,o');
    expect(ecommerce).toContain('lock ? " FOR UPDATE" : ""');
    expect(ecommerce).toContain('nativeWrite(');
    for (const token of ['activeChannelAssertion', 'openCartAssertion', 'checkoutVersionAssertion', 'readSource', 'checkoutRow', 'cartScope']) {
      expect(ecommerce).toContain(token);
    }
  });
});

describe('Ecommerce read-tail r20 retirement check', () => {
  test('retirement:ts-statements === 38 (r19 BEFORE=42)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(38);
  });
});
