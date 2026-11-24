import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { assignmentReadSql } from '@erpx/server-identity_access/public/record-source-authority';
import { scopeAllowedSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe scope probes from HEAD. The landed-cost, count and
// transfer purchased-read gates shared one assignment/authority envelope and a
// NULL department scope; only the scope alias and the permission varied. These
// are byte-identity oracles only, never live query sources.
const sites: [scope: string, permission: string, file: string, arg: string][] = [
  ['landed_cost_scope', 'finance.read', 'landed-cost-purchases.ts', 'required'],
  ['count_scope', 'inventory.read', 'count-purchases.ts', 'permission'],
  ['transfer_scope', 'inventory.read', 'transfer-purchases.ts', 'permission'],
];
const preDedupe = (scope: string, permission: string) =>
  `SELECT ${assignmentReadSql(scope, permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) ${scope}`;

describe('Inventory read-tail r31 parameterized scope-allowed dedupe', () => {
  test('helper reproduces all three pre-dedupe literals byte-identically and stays read-only', () => {
    for (const [scope, permission] of sites) {
      const sql = scopeAllowedSql(scope, permission);
      expect(sql).toBe(preDedupe(scope, permission));
      expect(sql).toContain('SELECT ');
      expect(selectCount(sql)).toBe(selectCount(preDedupe(scope, permission)));
      expect(sql).not.toMatch(lockRe);
    }
  });

  test('r31 helper literal is defined exactly once in the canonical owner', () => {
    expect(src('read-tail-sql.ts').split('scopeAllowedSql =').length - 1).toBe(1);
  });

  test('former inline literals are gone and every call site composes the helper with unchanged binds', () => {
    const bindTail = '.bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId)';
    for (const [scope, , file, arg] of sites) {
      const text = src(file);
      expect(text).not.toContain('assignmentReadSql');
      expect(text).not.toContain('allowed FROM (SELECT ?::text company_id');
      expect(text).toContain(`database.prepare(scopeAllowedSql("${scope}",${arg}))`);
      expect(text).toContain(bindTail);
      expect((text.match(/scopeAllowedSql\(/g) ?? []).length).toBe(1); // 1 call site per file
    }
  });

  test('r11-r30 helpers, locks, count source adapters and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'packageObligationDocumentRowsSql', 'sourceScopeRowsSql',
      'scopeExistsSql', 'journalAccountLineExistsSql', 'childLineOwnerExistsSql', 'nativeRecordExistsSql',
      'branchScopeTupleSql', 'pendingLossSumSql', 'transitLossDecisionPendingSql',
      'evidencePositionRowsSql', 'recordIdExistsSql', 'childLineRowsSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    // Lock guards remain in their owning gates; the dedupe only touches the scope probe.
    expect(src('landed-cost-purchases.ts')).toContain('commerceLandedCostSourceReadLocks');
    expect(src('count-purchases.ts')).toContain('companyPurchaseReadLocks');
    expect(src('transfer-purchases.ts')).toContain('companyPurchaseReadLocks');
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r30 BEFORE=357 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r30 folded the 2x byte-identical child-line row selector into one helper
    // (358 -> 357). r31 folds the 3x parameterized current-assignment scope
    // probe into one helper (357 -> 353): six inline SELECT tokens removed, two
    // kept in the single canonical helper.
    expect(total).toBe(353);
    expect(total).toBeLessThan(357);
  });
});
