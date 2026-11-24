import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { attachmentSourceProofExistsSql, sourceRowsExistsSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const expectedSourceRows = (rows: string, alias: string, id: string, company: string, predicates: string) =>
  `EXISTS(SELECT 1 FROM (${rows}) ${alias} WHERE ${alias}.id=${id} AND ${alias}.company_id=${company} AND ${predicates})`;
const expectedSourceProof = (rows: string, alias: string, modules: string) =>
  `EXISTS(SELECT 1 FROM (${rows}) ${alias} WHERE ${alias}.id=? AND ${alias}.company_id=? AND ${alias}.source_proof::text=? AND ${modules})`;

describe('Inventory read-tail r21 parameterized EXISTS-family dedupe', () => {
  test('canonical helpers render byte-identical read-only SQL', () => {
    expect(sourceRowsExistsSql('<rows>', 'initial_source', 'id', 'company', '<preds>')).toBe(
      expectedSourceRows('<rows>', 'initial_source', 'id', 'company', '<preds>'));
    expect(attachmentSourceProofExistsSql('<rows>', 'service_source', '<modules>')).toBe(
      expectedSourceProof('<rows>', 'service_source', '<modules>'));
    for (const sql of [
      expectedSourceRows('<rows>', 'initial_source', 'id', 'company', '<preds>'),
      expectedSourceProof('<rows>', 'service_source', '<modules>'),
    ]) readOnly(sql);
  });

  test('r21 helper literals are defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('sourceRowsExistsSql =')).toHaveLength(2);
    expect(shared.split('attachmentSourceProofExistsSql =')).toHaveLength(2);
  });

  test('count-source EXISTS guards are composed, never inlined, in count-record-purchases', () => {
    const text = src('count-record-purchases.ts');
    expect(text).not.toContain('initial_source WHERE initial_source.id=${id}');
    expect(text).not.toContain('manufacturing_source WHERE manufacturing_source.id=${id}');
    expect(text.match(/sourceRowsExistsSql\(/g) ?? []).toHaveLength(4);
  });

  test('attachment source-proof EXISTS guards are composed, never inlined, in count-attachment-purchases', () => {
    const text = src('count-attachment-purchases.ts');
    expect(text).not.toContain('initial_source WHERE initial_source.id=?');
    expect(text).not.toContain('manufacturing_source WHERE manufacturing_source.id=?');
    expect(text.match(/attachmentSourceProofExistsSql\(/g) ?? []).toHaveLength(4);
  });

  test('retirement:ts-statements stays strictly below the r21 BEFORE=393 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(393);
  });
});
