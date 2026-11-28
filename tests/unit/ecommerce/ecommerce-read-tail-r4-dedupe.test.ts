import { describe, expect, test, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

const root = resolve(__dirname, '../../../modules/ecommerce/server/src');
const src = readFileSync(resolve(root, 'adapters/executive-metric-projections.ts'), 'utf8');
const rust = readFileSync(resolve(root, 'adapters/native_exec_metric/domain.rs'), 'utf8');

const scope = 'c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)';

describe('Ecommerce read-tail r4 count family, now native-owned', () => {
  test('the TS adapter no longer builds the count statement', () => {
    expect(src).not.toContain('ecommerceScalar');
    expect(src).not.toContain('countBy');
    expect(src).not.toContain('SELECT ');
    expect(src).toContain('nativeExecutiveMetric');
  });

  test('Rust still emits the four retired count statements byte-for-byte', () => {
    expect(rust).toContain(
      `SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE {scope_sql} AND c.status='open' GROUP BY 1 ORDER BY 1`,
    );
    expect(rust).toContain(
      `SELECT x.status label,count(*)::text value FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id WHERE {scope_sql} AND x.status IN ('confirmed','partially_fulfilled','fulfilled') AND x.created_at>=? AND x.created_at<? GROUP BY 1 ORDER BY 1`,
    );
    expect(rust).toContain(
      `SELECT r.status label,count(*)::text value FROM ecommerce_return_links r JOIN ecommerce_checkouts x ON x.id=r.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE {scope_sql} AND r.created_at>=? AND r.created_at<? GROUP BY 1 ORDER BY 1`,
    );
    expect(rust).toContain(
      `SELECT l.status label,count(*)::text value FROM ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON x.id=l.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE {scope_sql} AND l.created_at>=? AND l.created_at<? GROUP BY 1 ORDER BY 1`,
    );
    expect(rust).toContain(scope);
  });

  test('the native family stays SELECT-only', () => {
    const statements = rust.slice(0, rust.indexOf("#[cfg(test)]"));
    expect(statements).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});
