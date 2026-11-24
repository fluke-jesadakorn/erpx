import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { ecommerceExistsGuard } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';
import { commercialOwnerProjectionSources as commerceProjectionSources } from '../../../packages/server-commerce/src/public/commercial-owner-projection-sources';
import { commercialOwnerProjectionSources as financeProjectionSources } from '../../../packages/server-finance/src/public/commercial-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const src = read('adapters/ecommerce.ts');
const po = read('adapters/package-obligations.ts');
const shared = read('adapters/read-tail-sql.ts');

// Pre-dedupe SQL copied verbatim from HEAD (r18), kept only as byte-identity oracles.
const head = {
  syncFulfillment: `EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='ecommerce.checkout.sync-fulfillment' AND receipt.record_id=checkout.id AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'status'=checkout.status AND (receipt.result_json::jsonb->>'version')::bigint<=checkout.version)`,
  requestReturn: `NOT EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=checkout.id AND receipt.operation='ecommerce.checkout.request-return' AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'commerceReturnId'=link.commerce_return_id)`,
  listingCurrent: `EXISTS(SELECT 1 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND l.version=? AND l.status='published' AND c.status='active' AND c.company_id=? AND l.currency=?)`,
  commitReview: `EXISTS(SELECT 1 FROM ecommerce_channels WHERE id=? AND version=? AND status=? AND (?='retire' OR created_by<>?))`,
  productProjection: `EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_products}) commerce_products WHERE id=? AND company_id=? AND status='active')`,
  partyProjection: `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active')`,
};

// Frames rebuilt exactly as the adapters define them (r17 receipt frame, r14 exists frame).
const receiptAuditGuard = (kind: string, join: string, where: string) => `${kind}(SELECT 1 FROM command_receipts receipt${join} WHERE ${where})`;
const syncFulfillment = receiptAuditGuard('EXISTS', '', `receipt.operation='ecommerce.checkout.sync-fulfillment' AND receipt.record_id=checkout.id AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'status'=checkout.status AND (receipt.result_json::jsonb->>'version')::bigint<=checkout.version`);
const requestReturn = receiptAuditGuard('NOT EXISTS', '', `receipt.record_id=checkout.id AND receipt.operation='ecommerce.checkout.request-return' AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'commerceReturnId'=link.commerce_return_id`);
const activeProjectionAssertion = (source: string, alias: string, predicate: string) =>
  ecommerceExistsGuard(`(${source}) ${alias}`, `id=? AND company_id=? AND ${predicate}`);

describe('Ecommerce read-tail r19 Tier A dedupe', () => {
  test('r17 receipt frame rebuilds both checkout probes byte-identically', () => {
    expect(syncFulfillment).toBe(head.syncFulfillment);
    expect(requestReturn).toBe(head.requestReturn);
  });

  test('r14 exists frame rebuilds listing-current, commit-review and projection guards byte-identically', () => {
    expect(ecommerceExistsGuard('ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id', "l.id=? AND l.version=? AND l.status='published' AND c.status='active' AND c.company_id=? AND l.currency=?")).toBe(head.listingCurrent);
    expect(ecommerceExistsGuard('ecommerce_channels', "id=? AND version=? AND status=? AND (?='retire' OR created_by<>?)")).toBe(head.commitReview);
    expect(activeProjectionAssertion(commerceProjectionSources.commerce_products, 'commerce_products', "status='active'")).toBe(head.productProjection);
    expect(activeProjectionAssertion(financeProjectionSources.finance_parties, 'finance_parties', "kind='customer' AND status='active'")).toBe(head.partyProjection);
  });

  test('every guard composes from the shared frame at its call site; raw literals are gone', () => {
    for (const oracle of Object.values(head)) {
      expect(po.includes(oracle)).toBe(false);
      expect(src.includes(oracle)).toBe(false);
    }
    expect(po).toContain('const checkoutSyncGuard=receiptAuditGuard(');
    expect(po).toContain('const checkoutReturnGuard=receiptAuditGuard(');
    expect(po).toContain('AND ${checkoutSyncGuard},false)');
    expect(po).toContain('OR ${checkoutReturnGuard} FROM ecommerce_return_links');
    expect(po.match(/const checkoutSyncGuard=receiptAuditGuard\(/g) ?? []).toHaveLength(1);
    expect(po.match(/const checkoutReturnGuard=receiptAuditGuard\(/g) ?? []).toHaveLength(1);
    expect(src).toContain("assertion(write,key,'listing-current',ecommerceExistsGuard(");
    expect(src).toContain('ecommerceExistsGuard(table, "id=? AND version=? AND status=? AND (?=\'retire\' OR created_by<>?)")');
    expect(src).toContain('ecommerceExistsGuard(`(${source}) ${alias}`, `id=? AND company_id=? AND ${predicate}`)');
    expect((src.match(/ecommerceExistsGuard\(/g) ?? []).length).toBe(6);
    expect(shared.match(/export const ecommerceExistsGuard = /g) ?? []).toHaveLength(1);
    expect(po.match(/const receiptAuditGuard=/g) ?? []).toHaveLength(1);
    // r17 invariant: still exactly two in-template receipt-audit compositions.
    expect((po.match(/\$\{receiptAuditGuard\(/g) ?? []).length).toBe(2);
  });

  test('binds unchanged and guards stay read-only with no added locks', () => {
    expect((head.syncFulfillment.match(/\?/g) ?? []).length).toBe(0);
    expect((head.requestReturn.match(/\?/g) ?? []).length).toBe(0);
    expect((head.listingCurrent.match(/\?/g) ?? []).length).toBe(4);
    expect((head.commitReview.match(/\?/g) ?? []).length).toBe(5);
    expect((head.productProjection.match(/\?/g) ?? []).length).toBe(2);
    expect((head.partyProjection.match(/\?/g) ?? []).length).toBe(2);
    expect(src).toContain('[i.listingId,i.listingVersion,a.companyId,cart.currency]');
    expect(src).toContain('[id,i.expectedVersion,expected,i.action,a.userId]');
    for (const oracle of Object.values(head)) {
      expect(oracle).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(shared).not.toMatch(/FOR\s+UPDATE|LOCK\s+TABLE/i);
  });

  test('LOCK TABLE files, native plans and r1-r14 frame sites stay intact', () => {
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(po).toContain('salesCommerceSettlementSourceReadLocks(database)');
    expect(po).toContain('ecommerceCommerceSourceReadLocks(database)');
    expect(po).toContain('lockEcommerceFilePackageObligationSources(owner,lock)');
    expect(read('adapters/attachment-purchases.ts')).toContain('LOCK TABLE commerce_products IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
    expect(src).toContain('SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE');
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(src).toContain('lock ? " FOR UPDATE" : ""');
    expect(src).toContain('nativeWrite(');
    for (const token of ['activeChannelAssertion', 'openCartAssertion', 'checkoutVersionAssertion', 'readSource', 'checkoutRow', 'cartScope']) {
      expect(src).toContain(token);
    }
  });
});

describe('Ecommerce read-tail r19 retirement check', () => {
  test('retirement:ts-statements === 25 (r23 BEFORE=26; r22 BEFORE=29)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(25);
  });
});
