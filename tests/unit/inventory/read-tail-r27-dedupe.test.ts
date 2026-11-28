import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { packageObligationDocumentRowsSql } from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const docRows = (pending: string) =>
  `    UNION ALL SELECT 'transfer',t.id,t.version,t.status,t.total_value_minor${pending} FROM inventory_transfers t,scope WHERE t.company_id=scope.company_id AND t.status NOT IN ('posted','cancelled','rejected')\n` +
  `    UNION ALL SELECT 'transit',t.id,t.version,t.status,0${pending} FROM inventory_transit t,scope WHERE t.company_id=scope.company_id AND t.status NOT IN ('completed','cancelled')\n` +
  `    UNION ALL SELECT 'valuation',l.id,l.version,l.status,l.amount_minor${pending} FROM inventory_landed_costs l,scope WHERE l.company_id=scope.company_id AND l.status NOT IN ('posted','cancelled','rejected')`;

type DocCase = [string, string];
const docCases: DocCase[] = [['pos-package-obligations.ts', ',true'], ['pms-package-obligations.ts', '']];

describe('Inventory read-tail r27 package-obligation document row dedupe', () => {
  test('canonical helper renders byte-identical read-only SQL for every consumer', () => {
    for (const [, pending] of docCases) {
      expect(packageObligationDocumentRowsSql(pending)).toBe(docRows(pending));
      readOnly(packageObligationDocumentRowsSql(pending));
    }
  });

  test('r27 helper literal is defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('packageObligationDocumentRowsSql =')).toHaveLength(2);
  });

  test('document rows are composed, never inlined, in their adapters', () => {
    for (const [file, pending] of docCases) {
      const text = src(file);
      for (const inline of docRows(pending).split('\n')) expect(text).not.toContain(inline);
      expect(text).toContain(`packageObligationDocumentRowsSql('${pending}')`);
      expect(text.match(/packageObligationDocumentRowsSql\(/g) ?? []).toHaveLength(1);
    }
  });

  test('retirement:ts-statements stays strictly below the r27 BEFORE=366 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(366);
  });
});
