import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { sourceScopeRowsSql } from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

type ScopeCase = [string, string, string];
const scopeCases: ScopeCase[] = [
  ['${landedCostPurchaseSourceRows}', 's',
    'SELECT s.*,NULL::text department_id FROM (${landedCostPurchaseSourceRows}) s'],
  ['${qualityDisposalCountRows}', 'source',
    'SELECT source.*,NULL::text department_id FROM (${qualityDisposalCountRows}) source'],
];

describe('Inventory read-tail r26 source-row scope projection dedupe', () => {
  test('canonical helper renders byte-identical read-only SQL for every consumer', () => {
    for (const [rows, alias, expected] of scopeCases) {
      expect(sourceScopeRowsSql(rows, alias)).toBe(expected);
      readOnly(sourceScopeRowsSql(rows, alias));
    }
  });

  test('r26 helper literal is defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('sourceScopeRowsSql =')).toHaveLength(2);
  });

  test('source-row scope projections are composed, never inlined, in their adapters', () => {
    const landedRecord = src('landed-cost-record-purchases.ts');
    const landedAttachment = src('landed-cost-attachment-purchases.ts');
    const quality = src('quality-product-entry.ts');
    const [, , landedInline] = scopeCases[0]!;
    const [, , qualityInline] = scopeCases[1]!;
    for (const file of [landedRecord, landedAttachment]) {
      expect(file).not.toContain(landedInline);
      expect(file).toContain("sourceScopeRowsSql(landedCostPurchaseSourceRows,'s')");
    }
    expect(quality).not.toContain(qualityInline);
    expect(quality).toContain("sourceScopeRowsSql(qualityDisposalCountRows,'source')");
    expect(landedRecord.match(/sourceScopeRowsSql\(/g) ?? []).toHaveLength(1);
    expect(landedAttachment.match(/sourceScopeRowsSql\(/g) ?? []).toHaveLength(2);
    expect(quality.match(/sourceScopeRowsSql\(/g) ?? []).toHaveLength(1);
  });

  test('retirement:ts-statements stays strictly below the r26 BEFORE=369 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(369);
  });
});
