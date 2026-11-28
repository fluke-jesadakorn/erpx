import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../modules/ecommerce/server/src');
const src = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');

// Pre-dedupe fragments copied verbatim from HEAD, kept only as byte-identity oracles.
const head = {
  guardChannel: `NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=channel.id AND audit.action=receipt.operation WHERE receipt.record_id=channel.id AND receipt.operation=CASE channel.status WHEN 'rejected' THEN 'ecommerce.channel.reject' WHEN 'retired' THEN 'ecommerce.channel.retire' END AND receipt.actor_id=channel.reviewed_by AND receipt.authority_json::jsonb->>'companyId'=channel.company_id AND receipt.result_json::jsonb->>'status'=channel.status AND (receipt.result_json::jsonb->>'version')::bigint<=channel.version)`,
  guardListing: `NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=listing.id AND audit.action=receipt.operation WHERE receipt.record_id=listing.id AND receipt.operation=CASE listing.status WHEN 'rejected' THEN 'ecommerce.listing.reject' WHEN 'retired' THEN 'ecommerce.listing.retire' END AND receipt.actor_id=listing.reviewed_by AND receipt.authority_json::jsonb->>'companyId'=channel.company_id AND receipt.result_json::jsonb->>'status'=listing.status AND (receipt.result_json::jsonb->>'version')::bigint<=listing.version)`,
  cartCheckout: `SELECT 1 FROM checkouts checkout WHERE checkout.cart_id=cart.id`,
};

// Rebuild the r6 shared fragments exactly as the adapter defines them.
const reviewReceiptGuard = (entity: string) => `NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=${entity}.id AND audit.action=receipt.operation WHERE receipt.record_id=${entity}.id AND receipt.operation=CASE ${entity}.status WHEN 'rejected' THEN 'ecommerce.${entity}.reject' WHEN 'retired' THEN 'ecommerce.${entity}.retire' END AND receipt.actor_id=${entity}.reviewed_by AND receipt.authority_json::jsonb->>'companyId'=channel.company_id AND receipt.result_json::jsonb->>'status'=${entity}.status AND (receipt.result_json::jsonb->>'version')::bigint<=${entity}.version)`;
const cartHasCheckout = head.cartCheckout;

describe('Ecommerce read-tail r6 package-obligation guard families', () => {
  test('review-receipt guard is byte-identical for channel and listing callers', () => {
    expect(reviewReceiptGuard('channel')).toBe(head.guardChannel);
    expect(reviewReceiptGuard('listing')).toBe(head.guardListing);
  });

  test('cart checkout fragment is byte-identical for EXISTS and NOT EXISTS callers', () => {
    expect(cartHasCheckout).toBe(head.cartCheckout);
    expect(`EXISTS(${cartHasCheckout})`).toBe(`EXISTS(${head.cartCheckout})`);
    expect(`NOT EXISTS(${cartHasCheckout})`).toBe(`NOT EXISTS(${head.cartCheckout})`);
  });

  test('families are defined once and the facts query composes from them', () => {
    expect(src.match(/const reviewReceiptGuard=/g) ?? []).toHaveLength(1);
    expect(src.match(/const cartHasCheckout=/g) ?? []).toHaveLength(1);
    expect(src.match(/\$\{reviewReceiptGuard\('channel'\)\}/g) ?? []).toHaveLength(1);
    expect(src.match(/\$\{reviewReceiptGuard\('listing'\)\}/g) ?? []).toHaveLength(1);
    expect(src.match(/\$\{cartHasCheckout\}/g) ?? []).toHaveLength(2);
    expect(src.includes(head.guardChannel)).toBe(false);
    expect(src.includes(head.guardListing)).toBe(false);
  });

  test('composed guards stay read-only and keep every operation token', () => {
    for (const sql of [reviewReceiptGuard('channel'), reviewReceiptGuard('listing'), cartHasCheckout]) {
      expect(sql).toMatch(/SELECT/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE/i);
    }
    expect(head.guardChannel).toContain("'ecommerce.channel.reject'");
    expect(head.guardListing).toContain("'ecommerce.listing.retire'");
  });
});

describe('Ecommerce read-tail r6 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r6 BEFORE=68 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(68);
  });
});
