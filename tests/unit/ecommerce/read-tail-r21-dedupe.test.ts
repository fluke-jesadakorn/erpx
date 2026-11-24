import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { ecommerceScopeRow, ecommerceReleaseGate } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const po = read('adapters/package-obligations.ts');
const ap = read('adapters/attachment-purchases.ts');
const pb = read('adapters/purchase-boundary.ts');
const shared = read('adapters/read-tail-sql.ts');

// Deterministic stubs resolve every interpolation on both sides of each byte oracle.
const reviewReceiptGuard = (entity: string) => `<review ${entity}>`;
const ecommerceExistsGuard = (table: string, where: string) => `<exists ${table} ${where}>`;
const receiptAuditGuard = (kind: string, join: string, where: string) => `<audit ${kind} ${join} ${where}>`;
const checkoutSyncGuard = '<sync>';
const checkoutReturnGuard = '<return>';
const cartHasCheckout = '<cart-checkout>';
const commerce = { products: '<products>', deliveries: '<deliveries>', orders: '<orders>', returns: '<returns>', warehouses: '<warehouses>' };
const finance = { finance_parties: '<parties>' };

// Frames rebuilt exactly as the adapters define them (r21).
const factBranch = (union: string, body: string) => `${union}SELECT ${body}`;

// Pre-r21 SQL copied verbatim from HEAD (r20), kept only as byte-identity oracles.
const headArms = [
  `SELECT 'channel' kind,channel.id,to_jsonb(channel) fact,channel.status NOT IN ('rejected','retired') OR ${reviewReceiptGuard('channel')} pending FROM ecommerce_channels channel,scope WHERE channel.company_id=scope.company_id`,
  `UNION ALL SELECT 'listing',listing.id,to_jsonb(listing),listing.status NOT IN ('rejected','retired') OR product.id IS NULL OR product.company_id<>channel.company_id OR ${reviewReceiptGuard('listing')} OR NOT ${ecommerceExistsGuard('money_transaction_snapshots captured', "captured.company_id=channel.company_id AND captured.source_type='ecommerce_listing' AND captured.source_id=listing.id AND captured.amount_kind='unit_price' AND captured.currency=listing.currency AND captured.amount_units=listing.price_minor")} FROM ecommerce_listings listing JOIN ecommerce_channels channel ON channel.id=listing.channel_id LEFT JOIN (${commerce.products}) product ON product.id=listing.product_id CROSS JOIN scope WHERE channel.company_id=scope.company_id`,
  `UNION ALL SELECT 'cart',cart.id,to_jsonb(cart),cart.status='open' OR channel.company_id IS DISTINCT FROM cart.company_id OR party.company_id IS DISTINCT FROM cart.company_id OR party.kind IS DISTINCT FROM 'customer' OR warehouse.company_id IS DISTINCT FROM cart.company_id OR warehouse.branch_id IS DISTINCT FROM cart.branch_id OR (cart.status='abandoned' AND (EXISTS(${cartHasCheckout}) OR ${receiptAuditGuard('NOT EXISTS',` JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=cart.id AND audit.record_version=cart.version AND audit.action=receipt.operation`,`receipt.record_id=cart.id AND receipt.operation='ecommerce.cart.abandon' AND receipt.authority_json::jsonb->>'companyId'=cart.company_id AND receipt.result_json::jsonb->>'status'='abandoned' AND receipt.result_json::jsonb->>'version'=cart.version::text`)})) OR cart.status='checked_out' AND NOT EXISTS(${cartHasCheckout}) FROM carts cart LEFT JOIN ecommerce_channels channel ON channel.id=cart.channel_id LEFT JOIN (${finance.finance_parties}) party ON party.id=cart.party_id LEFT JOIN (${commerce.warehouses}) warehouse ON warehouse.id=cart.warehouse_id`,
  `UNION ALL SELECT 'cart-line',line.id,to_jsonb(line),listing.id IS NULL OR listing.channel_id<>cart.channel_id OR listing.product_id<>line.product_id OR listing.currency<>cart.currency OR listing.price_minor<>line.price_minor OR line.listing_version>listing.version OR line.listing_version<1 OR line.quantity_milli<=0 FROM ecommerce_cart_lines line JOIN carts cart ON cart.id=line.cart_id LEFT JOIN ecommerce_listings listing ON listing.id=line.listing_id`,
  `UNION ALL SELECT 'checkout',checkout.id,to_jsonb(checkout),NOT COALESCE(checkout.id='ecommerce-checkout-'||checkout.cart_id AND checkout.sales_order_id='order-ecommerce-order-'||checkout.cart_id AND original.id=checkout.sales_order_id AND original.company_id=checkout.company_id AND original.kind='sales' AND original.branch_id=checkout.branch_id AND original.department_id IS NOT DISTINCT FROM checkout.department_id AND original.party_id=checkout.party_id AND original.warehouse_id=checkout.warehouse_id AND original.currency=checkout.currency AND checkout.current_cart_version=checkout.cart_version+1 AND checkout.sales_order_version<=original.version AND checkout.status=original.state AND checkout.status IN ('cancelled','fulfilled') AND ${receiptAuditGuard('EXISTS',` JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=checkout.id AND audit.action=receipt.operation`,`receipt.operation='ecommerce.checkout' AND receipt.record_id=checkout.id AND receipt.actor_id=checkout.created_by AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'cartId'=checkout.cart_id AND receipt.result_json::jsonb->>'salesOrderId'=checkout.sales_order_id`)} AND ${checkoutSyncGuard},false) OR ${ecommerceExistsGuard(`(${commerce.deliveries}) delivery`, `delivery.order_id=checkout.sales_order_id AND NOT ${ecommerceExistsGuard('ecommerce_fulfillment_links link', 'link.checkout_id=checkout.id AND link.fulfillment_id=delivery.id')}`)} FROM checkouts checkout LEFT JOIN (${commerce.orders}) original ON original.id=checkout.sales_order_id`,
  `UNION ALL SELECT 'fulfillment-link',link.id,to_jsonb(link),delivery.id IS NULL OR delivery.order_id<>checkout.sales_order_id OR link.source_order_version>original.version OR link.status NOT IN ('partially_fulfilled','fulfilled') FROM ecommerce_fulfillment_links link JOIN checkouts checkout ON checkout.id=link.checkout_id LEFT JOIN (${commerce.deliveries}) delivery ON delivery.id=link.fulfillment_id LEFT JOIN (${commerce.orders}) original ON original.id=checkout.sales_order_id`,
  `UNION ALL SELECT 'return-link',link.id,to_jsonb(link),returned.id IS NULL OR returned.order_id<>checkout.sales_order_id OR returned.fulfillment_id<>link.fulfillment_id OR delivery.order_id<>checkout.sales_order_id OR link.source_fulfillment_version>delivery.version OR ${checkoutReturnGuard} FROM ecommerce_return_links link JOIN checkouts checkout ON checkout.id=link.checkout_id LEFT JOIN (${commerce.returns}) returned ON returned.id=link.commerce_return_id LEFT JOIN (${commerce.deliveries}) delivery ON delivery.id=link.fulfillment_id ORDER BY kind,id`,
];

// The seven fact arms now compose from the one r21 frame.
const rebuiltArms = [
  factBranch('',`'channel' kind,channel.id,to_jsonb(channel) fact,channel.status NOT IN ('rejected','retired') OR ${reviewReceiptGuard('channel')} pending FROM ecommerce_channels channel,scope WHERE channel.company_id=scope.company_id`),
  factBranch('UNION ALL ',`'listing',listing.id,to_jsonb(listing),listing.status NOT IN ('rejected','retired') OR product.id IS NULL OR product.company_id<>channel.company_id OR ${reviewReceiptGuard('listing')} OR NOT ${ecommerceExistsGuard('money_transaction_snapshots captured', "captured.company_id=channel.company_id AND captured.source_type='ecommerce_listing' AND captured.source_id=listing.id AND captured.amount_kind='unit_price' AND captured.currency=listing.currency AND captured.amount_units=listing.price_minor")} FROM ecommerce_listings listing JOIN ecommerce_channels channel ON channel.id=listing.channel_id LEFT JOIN (${commerce.products}) product ON product.id=listing.product_id CROSS JOIN scope WHERE channel.company_id=scope.company_id`),
  factBranch('UNION ALL ',`'cart',cart.id,to_jsonb(cart),cart.status='open' OR channel.company_id IS DISTINCT FROM cart.company_id OR party.company_id IS DISTINCT FROM cart.company_id OR party.kind IS DISTINCT FROM 'customer' OR warehouse.company_id IS DISTINCT FROM cart.company_id OR warehouse.branch_id IS DISTINCT FROM cart.branch_id OR (cart.status='abandoned' AND (EXISTS(${cartHasCheckout}) OR ${receiptAuditGuard('NOT EXISTS',` JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=cart.id AND audit.record_version=cart.version AND audit.action=receipt.operation`,`receipt.record_id=cart.id AND receipt.operation='ecommerce.cart.abandon' AND receipt.authority_json::jsonb->>'companyId'=cart.company_id AND receipt.result_json::jsonb->>'status'='abandoned' AND receipt.result_json::jsonb->>'version'=cart.version::text`)})) OR cart.status='checked_out' AND NOT EXISTS(${cartHasCheckout}) FROM carts cart LEFT JOIN ecommerce_channels channel ON channel.id=cart.channel_id LEFT JOIN (${finance.finance_parties}) party ON party.id=cart.party_id LEFT JOIN (${commerce.warehouses}) warehouse ON warehouse.id=cart.warehouse_id`),
  factBranch('UNION ALL ',`'cart-line',line.id,to_jsonb(line),listing.id IS NULL OR listing.channel_id<>cart.channel_id OR listing.product_id<>line.product_id OR listing.currency<>cart.currency OR listing.price_minor<>line.price_minor OR line.listing_version>listing.version OR line.listing_version<1 OR line.quantity_milli<=0 FROM ecommerce_cart_lines line JOIN carts cart ON cart.id=line.cart_id LEFT JOIN ecommerce_listings listing ON listing.id=line.listing_id`),
  factBranch('UNION ALL ',`'checkout',checkout.id,to_jsonb(checkout),NOT COALESCE(checkout.id='ecommerce-checkout-'||checkout.cart_id AND checkout.sales_order_id='order-ecommerce-order-'||checkout.cart_id AND original.id=checkout.sales_order_id AND original.company_id=checkout.company_id AND original.kind='sales' AND original.branch_id=checkout.branch_id AND original.department_id IS NOT DISTINCT FROM checkout.department_id AND original.party_id=checkout.party_id AND original.warehouse_id=checkout.warehouse_id AND original.currency=checkout.currency AND checkout.current_cart_version=checkout.cart_version+1 AND checkout.sales_order_version<=original.version AND checkout.status=original.state AND checkout.status IN ('cancelled','fulfilled') AND ${receiptAuditGuard('EXISTS',` JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=checkout.id AND audit.action=receipt.operation`,`receipt.operation='ecommerce.checkout' AND receipt.record_id=checkout.id AND receipt.actor_id=checkout.created_by AND receipt.authority_json::jsonb->>'companyId'=checkout.company_id AND receipt.result_json::jsonb->>'cartId'=checkout.cart_id AND receipt.result_json::jsonb->>'salesOrderId'=checkout.sales_order_id`)} AND ${checkoutSyncGuard},false) OR ${ecommerceExistsGuard(`(${commerce.deliveries}) delivery`, `delivery.order_id=checkout.sales_order_id AND NOT ${ecommerceExistsGuard('ecommerce_fulfillment_links link', 'link.checkout_id=checkout.id AND link.fulfillment_id=delivery.id')}`)} FROM checkouts checkout LEFT JOIN (${commerce.orders}) original ON original.id=checkout.sales_order_id`),
  factBranch('UNION ALL ',`'fulfillment-link',link.id,to_jsonb(link),delivery.id IS NULL OR delivery.order_id<>checkout.sales_order_id OR link.source_order_version>original.version OR link.status NOT IN ('partially_fulfilled','fulfilled') FROM ecommerce_fulfillment_links link JOIN checkouts checkout ON checkout.id=link.checkout_id LEFT JOIN (${commerce.deliveries}) delivery ON delivery.id=link.fulfillment_id LEFT JOIN (${commerce.orders}) original ON original.id=checkout.sales_order_id`),
  factBranch('UNION ALL ',`'return-link',link.id,to_jsonb(link),returned.id IS NULL OR returned.order_id<>checkout.sales_order_id OR returned.fulfillment_id<>link.fulfillment_id OR delivery.order_id<>checkout.sales_order_id OR link.source_fulfillment_version>delivery.version OR ${checkoutReturnGuard} FROM ecommerce_return_links link JOIN checkouts checkout ON checkout.id=link.checkout_id LEFT JOIN (${commerce.returns}) returned ON returned.id=link.commerce_return_id LEFT JOIN (${commerce.deliveries}) delivery ON delivery.id=link.fulfillment_id ORDER BY kind,id`),
];

// Pre-r21 release/feature gates copied verbatim from HEAD (r20).
const moduleAssignment = '<module>';
const featureFile = '<feature-file>';
const featureFileRelease = '<feature-file-release>';
const featureRelease = '<feature-release>';
const headGates = [
  `SELECT EXISTS(${moduleAssignment}) AND ${featureFile} allowed FROM (SELECT ?::text company_id) ecommerce_file`,
  `(SELECT ${featureFileRelease} FROM (SELECT ?::text company_id) ecommerce_file_release)`,
  `SELECT ${featureRelease} allowed FROM (SELECT ?::text company_id) ecommerce_release`,
];
const rebuiltGates = [
  ecommerceReleaseGate(ecommerceScopeRow('ecommerce_file'), `EXISTS(${moduleAssignment}) AND ${featureFile}`),
  `(${ecommerceReleaseGate(ecommerceScopeRow('ecommerce_file_release'), featureFileRelease, '')})`,
  ecommerceReleaseGate(ecommerceScopeRow('ecommerce_release'), featureRelease),
];

describe('Ecommerce read-tail r21 fact-arm, gate and scope-row dedupe', () => {
  test('F1: seven UNION fact arms rebuild byte-identically from one frame', () => {
    expect(rebuiltArms).toEqual(headArms);
    expect(rebuiltArms).toHaveLength(7);
    expect(rebuiltArms[0]).not.toContain('UNION ALL');
    for (const arm of rebuiltArms.slice(1)) expect(arm.startsWith('UNION ALL SELECT ')).toBe(true);
  });

  test('F2: three release/feature gate frames rebuild byte-identically', () => {
    expect(rebuiltGates).toEqual(headGates);
    expect(rebuiltGates[1].startsWith('(') && rebuiltGates[1].endsWith(')')).toBe(true);
  });

  test('F3: scope-row separator frame keeps both column shapes and custom separators', () => {
    expect(ecommerceScopeRow('ecommerce_release')).toBe('(SELECT ?::text company_id) ecommerce_release');
    expect(ecommerceScopeRow('ecommerce_file_review', true)).toBe('(SELECT ?::text company_id,?::text branch_id,?::text department_id) ecommerce_file_review');
    expect(ecommerceScopeRow('workforce_actor_scope', true, '')).toBe('(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope');
  });

  test('raw literals are gone and each family composes exactly once from its frame', () => {
    expect((po.match(/const factBranch = /g) ?? []).length).toBe(1);
    expect((po.match(/\$\{factBranch\(/g) ?? []).length).toBe(7);
    expect(po).not.toContain('UNION ALL SELECT ');
    expect(po).not.toContain(" SELECT 'channel' kind");
    expect((shared.match(/export const ecommerceReleaseGate = /g) ?? []).length).toBe(1);
    expect((shared.match(/export const ecommerceScopeRow = /g) ?? []).length).toBe(1);
    expect((ap.match(/ecommerceReleaseGate\(/g) ?? []).length).toBe(3);
    expect((pb.match(/ecommerceReleaseGate\(/g) ?? []).length).toBe(1);
    // Prior scope-row call-site counts are unchanged.
    expect((ap.match(/ecommerceScopeRow\(/g) ?? []).length).toBe(5);
    expect((pb.match(/ecommerceScopeRow\(/g) ?? []).length).toBe(1);
    expect(ap).toContain('ecommerceReleaseGate(ecommerceScopeRow(');
    expect(pb).toContain('ecommerceReleaseGate(ecommerceScopeRow(');
  });

  test('binds and locks stay unchanged; composed frames stay read-only', () => {
    const cte = po.split('\n').find(l => l.includes('WITH scope AS'))!;
    expect(cte).toContain("WITH scope AS${ecommerceScopeRow('',false,'')}");
    expect((`WITH scope AS${ecommerceScopeRow('',false,'')}`.match(/\?/g) ?? []).length).toBe(1);
    expect((ecommerceScopeRow('ecommerce_file').match(/\?/g) ?? []).length).toBe(1);
    expect((ecommerceScopeRow('ecommerce_file', true).match(/\?/g) ?? []).length).toBe(3);
    expect(po).toContain('.bind(input.companyId)');
    expect(ap).toContain('.bind(actor.assignmentId,actor.companyId,actor.companyId)');
    expect(pb).toContain('.bind(actor.companyId)');
    for (const sql of [...headArms, ...headGates]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(ap).toContain('LOCK TABLE commerce_products IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
  });

  test('native code and r1-r20 frame sites stay intact', () => {
    for (const token of ['ecommerceModuleAssignment', 'ecommerceScopeRow', 'ecommerceExistsGuard', 'ecommerceOrderedList', 'ecommerceListingChannelRow', 'ecommerceSourceIdentity', 'ecommerceListingEvidence', 'ecommerceReleaseGate']) {
      expect(shared).toContain(`export const ${token} =`);
    }
    expect(po).toContain('const receiptAuditGuard=');
    expect(po).toContain('const cartHasCheckout=');
    expect(po).toContain("${reviewReceiptGuard('listing')}");
    expect(po).toContain('AND ${checkoutSyncGuard},false)');
    expect(po).toContain('OR ${checkoutReturnGuard} FROM ecommerce_return_links');
    expect((po.match(/\$\{receiptAuditGuard\(/g) ?? []).length).toBe(2);
    const src = read('adapters/ecommerce.ts');
    expect(src).toContain('SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE');
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(src).toContain('nativeWrite(');
  });
});

describe('Ecommerce read-tail r21 retirement check', () => {
  test('retirement:ts-statements === 19 (r25 BEFORE=22; r24 BEFORE=25)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(11);
  });
});
