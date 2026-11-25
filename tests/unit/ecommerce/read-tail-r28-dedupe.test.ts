import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

import { ecommerceScalar } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';
import { ecommerceNativeOrderRoots } from '../../../packages/server-ecommerce/src/adapters/package-sources';
import { readExecutiveMetricProjection } from '../../../packages/server-ecommerce/src/adapters/executive-metric-projections';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const src = read('adapters/ecommerce.ts');
const po = read('adapters/package-obligations.ts');
const emp = read('adapters/executive-metric-projections.ts');
const ps = read('adapters/package-sources.ts');

// Render the fact-arm frame from the actual r28 source so the byte oracle
// covers the folded definition, not a copy of it.
const defLine = po.split('\n').find(l => l.includes('const factBranch = '))!;
const factExpr = defLine.slice(defLine.indexOf('=> ') + 3).trim().replace(/;$/, '');
const factBranch = new Function('ecommerceScalar', `return (union, body) => ${factExpr};`)(ecommerceScalar) as (union: string, body: string) => string;

const scope = 'c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)';
const actor = { companyId: 'c', branchId: null, departmentId: null, modules: ['ecommerce'], permissions: [], userId: 'u', holdingId: 'h' } as any;
const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'THB' } as any;

async function sqlFor(id: string): Promise<string> {
  let sql = '';
  const db: any = { prepare(statement: string) { sql = statement; return { bind() { return this; }, all() { return { results: [] }; } }; } };
  await readExecutiveMetricProjection(db, actor, id as any, filters, 'status' as any);
  return sql;
}

describe('Ecommerce read-tail r28 scalar-head fold', () => {
  test('factBranch folds into ecommerceScalar and emits byte-identical UNION arms', () => {
    expect(po.match(/const factBranch = /g) ?? []).toHaveLength(1);
    expect(factExpr).toBe('`${union}${ecommerceScalar(body)}`');
    expect(po).not.toContain('`${union}SELECT ${body}`');
    for (const body of [
      `'channel' kind,channel.id,to_jsonb(channel) fact FROM t WHERE id=?`,
      `'listing',listing.id,to_jsonb(listing) FROM t WHERE company_id=? AND status<>'retired'`,
      `'return-link',link.id,to_jsonb(link) FROM t WHERE created_at>=? AND created_at<? ORDER BY kind,id`,
    ]) {
      const union = 'UNION ALL ';
      expect(factBranch('', body)).toBe(`SELECT ${body}`);
      expect(factBranch(union, body)).toBe(`${union}SELECT ${body}`);
      expect(factBranch(union, body)).toBe(`${union}${ecommerceScalar(body)}`);
    }
    expect((po.match(/\$\{factBranch\(/g) ?? []).length).toBe(7);
  });

  test('countBy emits the four pre-r28 SQL strings byte-for-byte through ecommerceScalar', async () => {
    expect(emp.match(/const countBy = /g) ?? []).toHaveLength(1);
    expect(emp).toContain('ecommerceScalar(`${label} label,count(*)::text value FROM ${relation} WHERE ${scopedAlias("c")} AND ${extra} GROUP BY 1 ORDER BY 1`)');
    expect(emp).not.toContain('`SELECT ${label} label,count(*)::text value');
    expect(await sqlFor('open-ecommerce-carts')).toBe(
      `SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE ${scope} AND c.status='open' GROUP BY 1 ORDER BY 1`,
    );
    expect(await sqlFor('completed-ecommerce-checkouts')).toBe(
      `SELECT x.status label,count(*)::text value FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id WHERE ${scope} AND x.status IN ('confirmed','partially_fulfilled','fulfilled') AND x.created_at>=? AND x.created_at<? GROUP BY 1 ORDER BY 1`,
    );
    expect(await sqlFor('ecommerce-return-links')).toBe(
      `SELECT r.status label,count(*)::text value FROM ecommerce_return_links r JOIN ecommerce_checkouts x ON x.id=r.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE ${scope} AND r.created_at>=? AND r.created_at<? GROUP BY 1 ORDER BY 1`,
    );
    expect(await sqlFor('ecommerce-fulfillment-links')).toBe(
      `SELECT l.status label,count(*)::text value FROM ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON x.id=l.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE ${scope} AND l.created_at>=? AND l.created_at<? GROUP BY 1 ORDER BY 1`,
    );
  });

  test('ecommerceNativeOrderRoots keeps its static projection byte-identical', () => {
    expect(ps).toContain("export const ecommerceNativeOrderRoots=ecommerceScalar(`cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart`);");
    expect(ecommerceNativeOrderRoots).toBe("SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart");
    expect(ecommerceNativeOrderRoots).toHaveLength(107);
    expect((ecommerceNativeOrderRoots.match(/\?/g) ?? []).length).toBe(0);
  });

  test('protected FOR UPDATE and LOCK TABLE families stay untouched', () => {
    expect(src).toContain("ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')");
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Ecommerce read-tail r28 retirement check', () => {
  test('KPI: ts SELECT tokens === 5 (r29 BEFORE=7)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(1);
    expect(total).toBeLessThan(10);
  });
});
