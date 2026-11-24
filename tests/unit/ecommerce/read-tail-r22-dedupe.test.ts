import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { ecommerceSelectOne, ecommerceModuleAssignment, ecommerceExistsGuard } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const shared = read('adapters/read-tail-sql.ts');
const po = read('adapters/package-obligations.ts');

// Pre-r22 SQL copied verbatim from HEAD (r21), kept only as byte-identity oracles.
const head = {
  moduleBare: 'SELECT 1 FROM assignments WHERE id=? AND company_id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> \'["ecommerce"]\'::jsonb)',
  moduleAlias: 'SELECT 1 FROM assignments assignment WHERE assignment.id=? AND assignment.company_id=? AND (jsonb_array_length(assignment.modules_json::jsonb)=0 OR assignment.modules_json::jsonb @> \'["ecommerce"]\'::jsonb)',
  existsListing: 'EXISTS(SELECT 1 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=? AND l.version=?)',
  existsGeneric: (table: string) => `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
  receiptChannel: `NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=channel.id AND audit.action=receipt.operation WHERE receipt.record_id=channel.id AND receipt.operation=CASE channel.status WHEN 'rejected' THEN 'ecommerce.channel.reject' WHEN 'retired' THEN 'ecommerce.channel.retire' END AND receipt.actor_id=channel.reviewed_by AND receipt.authority_json::jsonb->>'companyId'=channel.company_id AND receipt.result_json::jsonb->>'status'=channel.status AND (receipt.result_json::jsonb->>'version')::bigint<=channel.version)`,
  receiptCheckout: `EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='ecommerce.checkout.sync-fulfillment' AND receipt.record_id=checkout.id AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'status'=checkout.status AND (receipt.result_json::jsonb->>'version')::bigint<=checkout.version)`,
  cartCheckout: 'SELECT 1 FROM checkouts checkout WHERE checkout.cart_id=cart.id',
};

// The two package-obligation frames rebuilt exactly as the adapter defines them (r22).
const receiptAuditGuard = (kind: string, join: string, where: string) =>
  `${kind}(${ecommerceSelectOne(`FROM command_receipts receipt${join} WHERE ${where}`)})`;
const cartHasCheckout = ecommerceSelectOne('FROM checkouts checkout WHERE checkout.cart_id=cart.id');

describe('Ecommerce read-tail r22 select-one probe dedupe', () => {
  test('F1: one-row probe frame rebuilds the module assignment and EXISTS heads byte-identically', () => {
    expect(ecommerceSelectOne('FROM x WHERE y=?')).toBe('SELECT 1 FROM x WHERE y=?');
    expect(ecommerceModuleAssignment()).toBe(head.moduleBare);
    expect(ecommerceModuleAssignment('assignment')).toBe(head.moduleAlias);
    expect(ecommerceExistsGuard('ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id', 'l.id=? AND c.company_id=? AND l.version=?')).toBe(head.existsListing);
    expect(ecommerceExistsGuard('ecommerce_channels', 'id=? AND company_id=? AND version=?')).toBe(head.existsGeneric('ecommerce_channels'));
  });

  test('F1/F2: package-obligation probes rebuild byte-identically from the same frame', () => {
    expect(receiptAuditGuard('NOT EXISTS', ' JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=channel.id AND audit.action=receipt.operation', "receipt.record_id=channel.id AND receipt.operation=CASE channel.status WHEN 'rejected' THEN 'ecommerce.channel.reject' WHEN 'retired' THEN 'ecommerce.channel.retire' END AND receipt.actor_id=channel.reviewed_by AND receipt.authority_json::jsonb->>'companyId'=channel.company_id AND receipt.result_json::jsonb->>'status'=channel.status AND (receipt.result_json::jsonb->>'version')::bigint<=channel.version")).toBe(head.receiptChannel);
    expect(receiptAuditGuard('EXISTS', '', "receipt.operation='ecommerce.checkout.sync-fulfillment' AND receipt.record_id=checkout.id AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'status'=checkout.status AND (receipt.result_json::jsonb->>'version')::bigint<=checkout.version")).toBe(head.receiptCheckout);
    expect(cartHasCheckout).toBe(head.cartCheckout);
    expect(`EXISTS(${cartHasCheckout})`).toBe(`EXISTS(${head.cartCheckout})`);
    expect(`NOT EXISTS(${cartHasCheckout})`).toBe(`NOT EXISTS(${head.cartCheckout})`);
  });

  test('raw probe literals are gone and the frame composes from exactly six sites', () => {
    expect((shared.match(/export const ecommerceSelectOne = /g) ?? []).length).toBe(1);
    expect((shared.match(/ecommerceSelectOne\(/g) ?? []).length).toBe(3);
    expect((po.match(/ecommerceSelectOne\(/g) ?? []).length).toBe(2);
    expect((shared.match(/export const ecommerceSelectOne = /g) ?? []).length + (shared.match(/ecommerceSelectOne\(/g) ?? []).length + (po.match(/ecommerceSelectOne\(/g) ?? []).length).toBe(6);
    expect(shared).not.toContain('SELECT 1 FROM assignments WHERE');
    expect(shared).not.toContain('EXISTS(SELECT 1 FROM ${table} WHERE ${where})');
    expect(po).not.toContain('(SELECT 1 FROM command_receipts receipt');
    expect(po).not.toContain("'SELECT 1 FROM checkouts checkout WHERE checkout.cart_id=cart.id'");
    expect(shared).toContain('return ecommerceSelectOne(`FROM assignments');
    expect(shared).toContain('`EXISTS(${ecommerceSelectOne(`FROM ${table} WHERE ${where}`)})`');
    expect(po).toContain('const receiptAuditGuard=(kind:string,join:string,where:string)=>`${kind}(${ecommerceSelectOne(');
    expect(po).toContain("const cartHasCheckout=ecommerceSelectOne('FROM checkouts checkout WHERE checkout.cart_id=cart.id');");
  });

  test('binds and locks stay unchanged; composed probes stay read-only', () => {
    expect((head.moduleBare.match(/\?/g) ?? []).length).toBe(2);
    expect((head.moduleAlias.match(/\?/g) ?? []).length).toBe(2);
    expect((head.existsListing.match(/\?/g) ?? []).length).toBe(3);
    expect((head.existsGeneric('ecommerce_channels').match(/\?/g) ?? []).length).toBe(3);
    expect((head.receiptCheckout.match(/\?/g) ?? []).length).toBe(0);
    expect((head.cartCheckout.match(/\?/g) ?? []).length).toBe(0);
    expect(ecommerceModuleAssignment()).toBe(head.moduleBare);
    for (const sql of Object.values(head).filter(v => typeof v === 'string')) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(po).toContain('.bind(input.companyId)');
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(po).toContain('lockEcommerceFilePackageObligationSources(owner,lock)');
    expect(read('adapters/attachment-purchases.ts')).toContain('LOCK TABLE commerce_products IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
  });

  test('native, locks and r1-r21 frame sites stay intact', () => {
    for (const token of ['ecommerceModuleAssignment', 'ecommerceScopeRow', 'ecommerceReleaseGate', 'ecommerceExistsGuard', 'ecommerceOrderedList', 'ecommerceListingChannelRow', 'ecommerceSourceIdentity', 'ecommerceListingEvidence']) {
      expect(shared).toContain(`export const ${token} =`);
    }
    expect(po).toContain('const receiptAuditGuard=');
    expect(po).toContain('const cartHasCheckout=');
    expect(po).toContain('const factBranch = ');
    expect(po).toContain("import {ecommerceExistsGuard} from './read-tail-sql';");
    expect((po.match(/\$\{receiptAuditGuard\(/g) ?? []).length).toBe(2);
    expect((po.match(/\$\{cartHasCheckout\}/g) ?? []).length).toBe(2);
    const ecommerce = read('adapters/ecommerce.ts');
    expect(ecommerce).toContain('SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE');
    expect(ecommerce).toContain('FOR UPDATE OF x,c,o');
    expect(ecommerce).toContain('nativeWrite(');
  });
});

describe('Ecommerce read-tail r22 retirement check', () => {
  test('retirement:ts-statements === 22 (r24 BEFORE=25; r23 BEFORE=26)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(22);
  });
});
