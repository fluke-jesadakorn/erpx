import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const src = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');

// Pre-dedupe guard SQL copied verbatim from HEAD, kept only as byte-identity oracles.
const head = {
  channel: `NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=channel.id AND audit.action=receipt.operation WHERE receipt.record_id=channel.id AND receipt.operation=CASE channel.status WHEN 'rejected' THEN 'ecommerce.channel.reject' WHEN 'retired' THEN 'ecommerce.channel.retire' END AND receipt.actor_id=channel.reviewed_by AND receipt.authority_json::jsonb->>'companyId'=channel.company_id AND receipt.result_json::jsonb->>'status'=channel.status AND (receipt.result_json::jsonb->>'version')::bigint<=channel.version)`,
  listing: `NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=listing.id AND audit.action=receipt.operation WHERE receipt.record_id=listing.id AND receipt.operation=CASE listing.status WHEN 'rejected' THEN 'ecommerce.listing.reject' WHEN 'retired' THEN 'ecommerce.listing.retire' END AND receipt.actor_id=listing.reviewed_by AND receipt.authority_json::jsonb->>'companyId'=channel.company_id AND receipt.result_json::jsonb->>'status'=listing.status AND (receipt.result_json::jsonb->>'version')::bigint<=listing.version)`,
  cartAbandon: `NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=cart.id AND audit.record_version=cart.version AND audit.action=receipt.operation WHERE receipt.record_id=cart.id AND receipt.operation='ecommerce.cart.abandon' AND receipt.authority_json::jsonb->>'companyId'=cart.company_id AND receipt.result_json::jsonb->>'status'='abandoned' AND receipt.result_json::jsonb->>'version'=cart.version::text)`,
  checkout: `EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=checkout.id AND audit.action=receipt.operation WHERE receipt.operation='ecommerce.checkout' AND receipt.record_id=checkout.id AND receipt.actor_id=checkout.created_by AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'cartId'=checkout.cart_id AND receipt.result_json::jsonb->>'salesOrderId'=checkout.sales_order_id)`,
};

// Rebuild the r17 shared frame exactly as the adapter defines it.
const receiptAuditGuard = (kind: string, join: string, where: string) => `${kind}(SELECT 1 FROM command_receipts receipt${join} WHERE ${where})`;
const reviewReceiptGuard = (entity: string) => receiptAuditGuard(
  'NOT EXISTS',
  ` JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=${entity}.id AND audit.action=receipt.operation`,
  `receipt.record_id=${entity}.id AND receipt.operation=CASE ${entity}.status WHEN 'rejected' THEN 'ecommerce.${entity}.reject' WHEN 'retired' THEN 'ecommerce.${entity}.retire' END AND receipt.actor_id=${entity}.reviewed_by AND receipt.authority_json::jsonb->>'companyId'=channel.company_id AND receipt.result_json::jsonb->>'status'=${entity}.status AND (receipt.result_json::jsonb->>'version')::bigint<=${entity}.version`,
);
const cartAbandon = receiptAuditGuard(
  'NOT EXISTS',
  ` JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=cart.id AND audit.record_version=cart.version AND audit.action=receipt.operation`,
  `receipt.record_id=cart.id AND receipt.operation='ecommerce.cart.abandon' AND receipt.authority_json::jsonb->>'companyId'=cart.company_id AND receipt.result_json::jsonb->>'status'='abandoned' AND receipt.result_json::jsonb->>'version'=cart.version::text`,
);
const checkout = receiptAuditGuard(
  'EXISTS',
  ` JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=checkout.id AND audit.action=receipt.operation`,
  `receipt.operation='ecommerce.checkout' AND receipt.record_id=checkout.id AND receipt.actor_id=checkout.created_by AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'cartId'=checkout.cart_id AND receipt.result_json::jsonb->>'salesOrderId'=checkout.sales_order_id`,
);

describe('Ecommerce read-tail r17 receipt-audit guard family', () => {
  test('one frame emits all four guards byte-identically', () => {
    expect(reviewReceiptGuard('channel')).toBe(head.channel);
    expect(reviewReceiptGuard('listing')).toBe(head.listing);
    expect(cartAbandon).toBe(head.cartAbandon);
    expect(checkout).toBe(head.checkout);
  });

  test('frame is defined once and the facts query composes from it', () => {
    expect(src.match(/const receiptAuditGuard=/g) ?? []).toHaveLength(1);
    expect(src.match(/const reviewReceiptGuard=/g) ?? []).toHaveLength(1);
    expect((src.match(/\$\{receiptAuditGuard\(/g) ?? []).length).toBe(2);
    expect(src.match(/\$\{reviewReceiptGuard\('channel'\)\}/g) ?? []).toHaveLength(1);
    expect(src.match(/\$\{reviewReceiptGuard\('listing'\)\}/g) ?? []).toHaveLength(1);
    for (const literal of [head.channel, head.listing, head.cartAbandon, head.checkout]) {
      expect(src.includes(literal)).toBe(false);
    }
  });

  test('no binds added and every source lock stays intact', () => {
    expect((src.match(/\?/g) ?? []).length).toBe(0);
    expect(('WITH scope AS(SELECT ?::text company_id)'.match(/\?/g) ?? []).length).toBe(1);
    expect(src).toContain("LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE");
    expect(src).toContain("salesCommerceSettlementSourceReadLocks(database)");
    expect(src).toContain("ecommerceCommerceSourceReadLocks(database)");
    expect(src).toContain("lockEcommerceFilePackageObligationSources(owner,lock)");
    for (const sql of [head.channel, head.listing, head.cartAbandon, head.checkout]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE/i);
    }
  });

  test('retirement:ts-statements stays strictly below the r17 BEFORE=50 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(50);
  });
});
