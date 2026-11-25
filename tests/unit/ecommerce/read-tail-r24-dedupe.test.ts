import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import {
  ecommerceOrderedList,
  ecommerceRowProbe,
  ecommerceScopeRow,
  ecommerceSelectOne,
} from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const shared = read('adapters/read-tail-sql.ts');
const po = read('adapters/package-obligations.ts');
const src = read('adapters/ecommerce.ts');

const cartScope = 'c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)';
const products = '<products>';
const orders = '<orders>';

// Pre-r24 SQL copied verbatim from HEAD (r23), kept only as byte-identity oracles.
const head = {
  cte: 'WITH scope AS(SELECT ?::text company_id)',
  channel: 'SELECT * FROM ecommerce_channels WHERE company_id=? ORDER BY updated_at DESC',
  cartLine: 'SELECT * FROM ecommerce_cart_lines WHERE cart_id=? ORDER BY ordinal',
  fulfillmentLink: 'SELECT * FROM ecommerce_fulfillment_links WHERE checkout_id=?',
  listingList: `SELECT l.*,p.name product_name,c.name channel_name FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN (${products}) p ON p.id=l.product_id WHERE c.company_id=? ORDER BY l.updated_at DESC`,
  cartList: `SELECT c.* FROM ecommerce_carts c WHERE ${cartScope} ORDER BY c.updated_at DESC`,
  checkoutList: `SELECT x.*,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${orders}) o ON o.id=x.sales_order_id WHERE ${cartScope} ORDER BY x.updated_at DESC`,
  scopeRow: '(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope',
  assignmentProbe: 'SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE current.where',
  receiptProbe: "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?",
};

describe('Ecommerce read-tail r24 three-family dedupe', () => {
  test('F1: the CTE scope composes from ecommerceScopeRow byte-identically with one bind', () => {
    expect(ecommerceScopeRow('', false, '')).toBe('(SELECT ?::text company_id)');
    expect(`WITH scope AS${ecommerceScopeRow('', false, '')}`).toBe(head.cte);
    expect((head.cte.match(/\?/g) ?? []).length).toBe(1);
    const cte = po.split('\n').find(l => l.includes('WITH scope AS'))!;
    expect(cte).toContain("WITH scope AS${ecommerceScopeRow('',false,'')}");
    expect(po).not.toContain('WITH scope AS(SELECT ?::text company_id)');
    expect(po).toContain("import {ecommerceScopeRow,ecommerceOrderedList,ecommerceSelectOne} from './read-tail-sql';");
  });

  test('F2: the optional order tail rebuilds all three r11 source reads byte-identically', () => {
    expect(ecommerceOrderedList('*', 'ecommerce_channels', 'company_id=?', 'updated_at DESC')).toBe(head.channel);
    expect(ecommerceOrderedList('*', 'ecommerce_cart_lines', 'cart_id=?', 'ordinal')).toBe(head.cartLine);
    expect(ecommerceOrderedList('*', 'ecommerce_fulfillment_links', 'checkout_id=?')).toBe(head.fulfillmentLink);
    expect(ecommerceOrderedList('*', 't', 'x=?')).toBe('SELECT * FROM t WHERE x=?');
    expect(ecommerceOrderedList('*', 't', 'x=?', '')).toBe('SELECT * FROM t WHERE x=?');
  });

  test('F2: the r14 ordered-list outputs are unchanged', () => {
    expect(ecommerceOrderedList('l.*,p.name product_name,c.name channel_name', `ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN (${products}) p ON p.id=l.product_id`, 'c.company_id=?', 'l.updated_at DESC')).toBe(head.listingList);
    expect(ecommerceOrderedList('c.*', 'ecommerce_carts c', cartScope, 'c.updated_at DESC')).toBe(head.cartList);
    expect(ecommerceOrderedList('x.*,o.number,o.state order_state', `ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${orders}) o ON o.id=x.sales_order_id`, cartScope, 'x.updated_at DESC')).toBe(head.checkoutList);
  });

  test('F2: readSource routes the two remaining sites through the shared ordered-list frame', () => {
    expect(src).toContain("const readSource = ecommerceOrderedList.bind(null, '*');");
    expect((src.match(/const readSource = /g) ?? []).length).toBe(1);
    expect((src.match(/readSource\(/g) ?? []).length).toBe(2);
    expect((src.match(/ecommerceOrderedList\(/g) ?? []).length).toBe(5);
    expect(src).not.toContain('const readSource = (table: string, scope: string');
    expect(src).not.toContain('readSource("ecommerce_channels", "company_id=?", "updated_at DESC")');
    expect(src).toContain('readSource("ecommerce_cart_lines", "cart_id=?", "ordinal")');
    expect(src).toContain('readSource("ecommerce_fulfillment_links", "checkout_id=?")');
    const readSource = ecommerceOrderedList.bind(null, '*');
    expect(readSource('ecommerce_channels', 'company_id=?', 'updated_at DESC')).toBe(head.channel);
    expect(readSource('ecommerce_cart_lines', 'cart_id=?', 'ordinal')).toBe(head.cartLine);
    expect(readSource('ecommerce_fulfillment_links', 'checkout_id=?')).toBe(head.fulfillmentLink);
  });

  test('F3: the row probe composes from ecommerceSelectOne byte-identically', () => {
    expect(ecommerceScopeRow('workforce_actor_scope', true, '')).toBe(head.scopeRow);
    expect(ecommerceSelectOne('FROM x WHERE y=?')).toBe('SELECT 1 FROM x WHERE y=?');
    expect(ecommerceRowProbe(head.scopeRow, 'current.where', '')).toBe(head.assignmentProbe);
    expect(ecommerceRowProbe('command_receipts', "command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")).toBe(head.receiptProbe);
    expect(ecommerceRowProbe('x', 'y=?')).toBe(ecommerceSelectOne('FROM x WHERE y=?'));
    expect(shared).toContain('ecommerceSelectOne(`FROM${gap}${from} WHERE ${where}`)');
    expect((shared.match(/export const ecommerceSelectOne = /g) ?? []).length).toBe(1);
    expect((shared.match(/ecommerceSelectOne\(/g) ?? []).length).toBe(3);
    expect((po.match(/ecommerceSelectOne\(/g) ?? []).length).toBe(2);
  });

  test('F4: binds stay unchanged and composed frames stay read-only', () => {
    expect((head.channel.match(/\?/g) ?? []).length).toBe(1);
    expect((head.cartLine.match(/\?/g) ?? []).length).toBe(1);
    expect((head.fulfillmentLink.match(/\?/g) ?? []).length).toBe(1);
    expect((head.listingList.match(/\?/g) ?? []).length).toBe(1);
    expect((head.cartList.match(/\?/g) ?? []).length).toBe(5);
    expect((head.checkoutList.match(/\?/g) ?? []).length).toBe(5);
    expect((head.assignmentProbe.match(/\?/g) ?? []).length).toBe(3);
    expect((head.receiptProbe.match(/\?/g) ?? []).length).toBe(5);
    for (const sql of Object.values(head)) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(po).toContain('.bind(input.companyId)');
    expect(read('adapters/attachment-read.ts')).toContain('.bind(actor.companyId,actor.branchId,actor.departmentId,...current.args)');
    expect(read('adapters/purchase-boundary.ts')).toContain('.bind(command.key,command.operation,command.id,actor.userId,actor.companyId)');
  });

  test('F5: locks, native bridge and r1-r23 frame sites stay intact', () => {
    for (const token of [
      'ecommerceSelectOne', 'ecommerceModuleAssignment', 'ecommerceRowProbe', 'ecommerceScopeRow',
      'ecommerceReleaseGate', 'ecommerceExistsGuard', 'ecommerceOrderedList',
      'ecommerceListingChannelRow', 'ecommerceSourceIdentity', 'ecommerceListingEvidence',
    ]) {
      expect(shared).toContain(`export const ${token} =`);
    }
    expect(src).toContain("ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')");
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(src).toContain('lock ? " FOR UPDATE" : ""');
    expect((src.match(/FOR UPDATE/g) ?? []).length).toBe(3);
    expect(src).toContain('nativeWrite(');
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(read('adapters/attachment-purchases.ts')).toContain('LOCK TABLE commerce_products IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Ecommerce read-tail r24 retirement check', () => {
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
    expect(total).toBe(1);
    expect(total).toBeLessThan(22);
  });
});
