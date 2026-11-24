import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  terminalReceiptMatchSql,
  transitLossDecisionPendingSql,
} from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const terminalOwners = [
  'finance-record-sources.ts',
  'sales-package-obligations.ts',
  'supply-package-obligations.ts',
  'manufacturing-package-obligations.ts',
  'pos-package-obligations.ts',
] as const;
const transitOwners = ['inventory-transit.ts', 'supply-package-obligations.ts'] as const;

describe('Inventory read-tail r16 parameterized-EXISTS-family dedupe', () => {
  test('canonical helpers render the exact byte-identical SQL and stay read-only', () => {
    expect(terminalReceiptMatchSql('ROWS', 'count', 'c.id', 'c.company_id', 'c.status', 'c.version'))
      .toBe("EXISTS(SELECT 1 FROM (ROWS) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)");
    expect(terminalReceiptMatchSql('ROWS', 'landed-cost', 'cost.id', 'cost.company_id', 'cost.status', 'cost.version'))
      .toBe("EXISTS(SELECT 1 FROM (ROWS) terminal WHERE terminal.kind='landed-cost' AND terminal.id=cost.id AND terminal.company_id=cost.company_id AND terminal.status=cost.status AND terminal.version=cost.version)");
    expect(transitLossDecisionPendingSql('r', 'e.id'))
      .toBe('NOT EXISTS(SELECT 1 FROM inventory_transit_events r WHERE r.proposal_id=e.id)');
    expect(transitLossDecisionPendingSql('decision', 'p.id'))
      .toBe('NOT EXISTS(SELECT 1 FROM inventory_transit_events decision WHERE decision.proposal_id=p.id)');
    for (const sql of [
      terminalReceiptMatchSql('ROWS', 'count', 'c.id', 'c.company_id', 'c.status', 'c.version'),
      transitLossDecisionPendingSql('r', 'e.id'),
    ]) readOnly(sql);
  });

  test('each r16 literal is defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('EXISTS(SELECT 1 FROM (${rows}) terminal WHERE terminal.kind=\'${kind}\' AND terminal.id=${id} AND terminal.company_id=${company} AND terminal.status=${status} AND terminal.version=${version})')).toHaveLength(2);
    expect(shared.split('NOT EXISTS(SELECT 1 FROM inventory_transit_events ${alias} WHERE ${alias}.proposal_id=${id})')).toHaveLength(2);
  });

  test('terminal-receipt match is composed, never inlined, in every named adapter', () => {
    const inline = (rows: string) => `EXISTS(SELECT 1 FROM (${rows}) terminal WHERE terminal.kind=`;
    for (const name of terminalOwners) {
      const text = src(name);
      expect(text, name).not.toContain(inline('${supplyInventoryTerminalReceipts}'));
    }
    expect(src('finance-record-sources.ts').match(/terminalReceiptMatchSql/g) ?? []).toHaveLength(2);
    expect(src('sales-package-obligations.ts').match(/terminalReceiptMatchSql/g) ?? []).toHaveLength(2);
    expect(src('supply-package-obligations.ts').match(/terminalReceiptMatchSql/g) ?? []).toHaveLength(3);
    expect(src('manufacturing-package-obligations.ts').match(/terminalReceiptMatchSql/g) ?? []).toHaveLength(2);
    expect(src('pos-package-obligations.ts').match(/terminalReceiptMatchSql/g) ?? []).toHaveLength(2);
  });

  test('transit loss-decision guard is composed, never inlined, in every named adapter', () => {
    for (const name of transitOwners) {
      const text = src(name);
      expect(text, name).not.toMatch(/NOT EXISTS\(SELECT 1 FROM inventory_transit_events \w+ WHERE \w+\.proposal_id=/);
    }
    expect(src('inventory-transit.ts').match(/transitLossDecisionPendingSql/g) ?? []).toHaveLength(8);
    expect(src('supply-package-obligations.ts').match(/transitLossDecisionPendingSql/g) ?? []).toHaveLength(2);
  });

  test('retirement:ts-statements stays strictly below the r16 BEFORE=428 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(428);
  });
});
