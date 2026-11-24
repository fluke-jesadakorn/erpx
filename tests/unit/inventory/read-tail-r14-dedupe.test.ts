import { describe, expect, test } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  countInitialMarkedExistsSql,
  initialPhysicalCountJournalSql,
  initialPhysicalCountFinanceImagesSql,
} from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-inventory/src/adapters/${name}`, import.meta.url), 'utf8');

describe('Inventory read-tail r14 byte-identical dedupe', () => {
  test('canonical fragments own the exact shared text and stay read-only', () => {
    expect(countInitialMarkedExistsSql('ROWS', 'ID')).toBe('EXISTS(SELECT 1 FROM (ROWS) initial_marker WHERE initial_marker.id=ID)');
    expect(countInitialMarkedExistsSql('ROWS', 'ID', 'marker')).toBe('EXISTS(SELECT 1 FROM (ROWS) marker WHERE marker.id=ID)');
    expect(initialPhysicalCountJournalSql('ROWS', 'TAIL')).toContain("SELECT source.company_id,c.journal_id id FROM (ROWS) source JOIN inventory_counts c ON c.id=source.id");
    expect(initialPhysicalCountJournalSql('ROWS', 'TAIL').endsWith('TAIL')).toBe(true);
    expect(initialPhysicalCountFinanceImagesSql('IMAGES')).toBe('SELECT source.company_id,source.journal_id id,source.image FROM (IMAGES) source WHERE source.journal_id IS NOT NULL');
    for (const sql of [countInitialMarkedExistsSql('ROWS', 'ID'), initialPhysicalCountJournalSql('ROWS', 'TAIL'), initialPhysicalCountFinanceImagesSql('IMAGES')]) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  test('each r14 literal is defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('EXISTS(SELECT 1 FROM (${rows}) ${alias} WHERE ${alias}.id=${id})')).toHaveLength(2);
    expect(shared.split('SELECT source.company_id,c.journal_id id FROM (${rows}) source JOIN inventory_counts c ON c.id=source.id')).toHaveLength(2);
    expect(shared.split('SELECT source.company_id,source.journal_id id,source.image FROM (${images}) source WHERE source.journal_id IS NOT NULL')).toHaveLength(2);
  });

  test('count marker EXISTS alias variants are composed, never inlined', () => {
    const text = src('count-record-purchases.ts');
    expect(text).not.toContain('EXISTS(SELECT 1 FROM (${initialPhysicalStockMarkedRows})');
    expect(text).not.toContain('EXISTS(SELECT 1 FROM (${manufacturingInitialPhysicalStockMarkedRows})');
    expect(text).not.toContain('EXISTS(SELECT 1 FROM (${serviceInitialPhysicalStockMarkedRows})');
    expect(text).not.toContain('EXISTS(SELECT 1 FROM (${posInitialPhysicalStockMarkedRows})');
    expect(text.match(/countInitialMarkedExistsSql/g) ?? []).toHaveLength(5); // import + 4 callers
  });

  test('initial-count journals and finance images keep their shared tails and rows', () => {
    const text = src('finance-record-sources.ts');
    expect(text.match(/initialPhysicalCountJournalSql/g) ?? []).toHaveLength(5); // import + 4 callers (r46 adds sales)
    expect(text.match(/initialPhysicalCountFinanceImagesSql/g) ?? []).toHaveLength(4); // import + 3 callers
    expect(text.match(/initialPhysicalCountJournalTail/g) ?? []).toHaveLength(4); // definition + 3 args
    expect(text).not.toContain('`SELECT source.company_id,c.journal_id id FROM (${manufacturingInitialPhysicalStockCountSourceRows})');
    expect(text).not.toContain('`SELECT source.company_id,source.journal_id id,source.image FROM (${posInitialPhysicalStockSourceImages})');
  });
});
