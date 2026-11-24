import { describe, expect, test, vi } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import { readExecutiveMetricProjection } from '../../../packages/server-ecommerce/src/adapters/executive-metric-projections';

const src = readFileSync(
  new URL('../../../packages/server-ecommerce/src/adapters/executive-metric-projections.ts', import.meta.url),
  'utf8',
);

const actor = { companyId: 'c', branchId: null, departmentId: null, modules: ['ecommerce'], permissions: [], userId: 'u', holdingId: 'h' } as any;
const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'THB' } as any;
const scope = 'c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)';

async function sqlFor(id: string, breakdown: string): Promise<string> {
  let sql = '';
  const db: any = { prepare(statement: string) { sql = statement; return { bind() { return this; }, all() { return { results: [] }; } }; } };
  await readExecutiveMetricProjection(db, actor, id as any, filters, breakdown as any);
  return sql;
}

describe('Ecommerce read-tail r4 parameterized count family', () => {
  test('one owner literal for the count projection, three call sites', () => {
    expect(src.split('SELECT ${label} label,count(*)::text value')).toHaveLength(2);
    expect(src.match(/const countBy = /g) ?? []).toHaveLength(1);
    expect(src.match(/countBy\(/g) ?? []).toHaveLength(3);
  });

  test('open-carts SQL is byte-identical after parameterization', async () => {
    expect(await sqlFor('open-ecommerce-carts', 'status')).toBe(
      `SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE ${scope} AND c.status='open' GROUP BY 1 ORDER BY 1`,
    );
  });

  test('completed-checkouts SQL is byte-identical after parameterization', async () => {
    expect(await sqlFor('completed-ecommerce-checkouts', 'status')).toBe(
      `SELECT x.status label,count(*)::text value FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id WHERE ${scope} AND x.status IN ('confirmed','partially_fulfilled','fulfilled') AND x.created_at>=? AND x.created_at<? GROUP BY 1 ORDER BY 1`,
    );
  });

  test('return and fulfillment link SQL is byte-identical after parameterization', async () => {
    expect(await sqlFor('ecommerce-return-links', 'status')).toBe(
      `SELECT r.status label,count(*)::text value FROM ecommerce_return_links r JOIN ecommerce_checkouts x ON x.id=r.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE ${scope} AND r.created_at>=? AND r.created_at<? GROUP BY 1 ORDER BY 1`,
    );
    expect(await sqlFor('ecommerce-fulfillment-links', 'status')).toBe(
      `SELECT l.status label,count(*)::text value FROM ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON x.id=l.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE ${scope} AND l.created_at>=? AND l.created_at<? GROUP BY 1 ORDER BY 1`,
    );
  });

  test('projection SQL stays SELECT-only', async () => {
    for (const id of ['open-ecommerce-carts', 'completed-ecommerce-checkouts', 'ecommerce-return-links', 'ecommerce-fulfillment-links']) {
      const sql = await sqlFor(id, 'status');
      expect(sql).toMatch(/^SELECT /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });
});
