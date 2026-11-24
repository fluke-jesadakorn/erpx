import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { stockMoveSumSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const deliveryWhere = 'move.fulfillment_id=f.id AND move.product_id=line.product_id AND move.warehouse_id=f.warehouse_id';
const returnWhere = 'move.return_id=r.id AND move.product_id=line.product_id AND move.warehouse_id=r.warehouse_id';
const stockMoves = '<stockMoves>';
const returnStockMoves = '<returnStockMoves>';
const expected = [
  `COALESCE((SELECT sum(move.quantity_milli) FROM (${stockMoves}) move WHERE ${deliveryWhere}),0)`,
  `COALESCE((SELECT sum(move.value_minor) FROM (${stockMoves}) move WHERE ${deliveryWhere}),0)`,
  `COALESCE((SELECT sum(move.quantity_milli) FROM (${returnStockMoves}) move WHERE ${returnWhere}),0)`,
  `COALESCE((SELECT sum(move.value_minor) FROM (${returnStockMoves}) move WHERE ${returnWhere}),0)`,
];

describe('Inventory read-tail r19 parameterized aggregate-family dedupe', () => {
  test('canonical helper renders byte-identical SQL and stays read-only', () => {
    expect(stockMoveSumSql('move.quantity_milli', stockMoves, deliveryWhere)).toBe(expected[0]);
    expect(stockMoveSumSql('move.value_minor', stockMoves, deliveryWhere)).toBe(expected[1]);
    expect(stockMoveSumSql('move.quantity_milli', returnStockMoves, returnWhere)).toBe(expected[2]);
    expect(stockMoveSumSql('move.value_minor', returnStockMoves, returnWhere)).toBe(expected[3]);
    for (const sql of expected) readOnly(sql);
  });

  test('r19 helper literal is defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('COALESCE((SELECT sum(${metric}) FROM (${moves}) move WHERE ${where}),0)')).toHaveLength(2);
  });

  test('settlement-move SUM is composed, never inlined, in the sales adapter', () => {
    const text = src('sales-package-obligations.ts');
    expect(text).not.toContain('COALESCE((SELECT sum(move.quantity_milli) FROM (${sales.stockMoves}) move WHERE ');
    expect(text).not.toContain('COALESCE((SELECT sum(move.value_minor) FROM (${sales.returnStockMoves}) move WHERE ');
    expect(text.match(/stockMoveSumSql\(/g) ?? []).toHaveLength(4);
  });

  test('retirement:ts-statements stays strictly below the r19 BEFORE=402 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(402);
  });
});
