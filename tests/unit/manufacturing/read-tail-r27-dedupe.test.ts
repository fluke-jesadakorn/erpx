import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import { manufacturingFinanceRecordSources } from '../../../packages/server-manufacturing/src/adapters/finance-record-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const journalPrefix = 'SELECT posting.company_id,posting.journal_id id';
const postingPrefix = 'SELECT posting.id posting_id,posting.company_id,posting.journal_id id';

/** The immutable owner posting and its accounting-profile variant were built
 * from the same FROM/JOIN/WHERE body with a string replace on the projection
 * prefix. One parameterized emitter now renders both byte for byte; this suite
 * pins that identity, the single definition and the untouched lock/helper set. */
describe('manufacturing read-tail r27 journal projection family', () => {
  it('emits both former statements byte-identically through one parameterized emitter', () => {
    const { journal, posting } = manufacturingFinanceRecordSources;
    expect(journal.startsWith(`${journalPrefix} FROM manufacturing_order_postings posting`)).toBe(true);
    expect(posting.startsWith(`${postingPrefix} FROM manufacturing_order_postings posting`)).toBe(true);
    expect(posting.slice(postingPrefix.length)).toBe(journal.slice(journalPrefix.length));
    expect(journal).toContain('JOIN finance_journals journal ON journal.id=posting.journal_id');
    for (const sql of [journal, posting]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only emitter and routes both callers through it', () => {
    const source = src('finance-record-sources.ts');
    expect(source.split('const manufacturingJournalSelect=')).toHaveLength(2);
    expect(source.split('manufacturingJournalSelect(').length - 1).toBe(2);
    expect(source).toContain('`SELECT ${columns} FROM manufacturing_order_postings posting');
    expect(source).not.toContain('.replace(');
    expect(source.match(/prepare\(/g) ?? []).toHaveLength(2); // only the two LOCK TABLE statements
  });

  it('leaves the r4/r11/r15-r26 helpers, locks and FOR UPDATE witness untouched', () => {
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingExistsSelect');
    expect(src('manufacturing-read-tail-r20-sql.ts')).toContain('export const manufacturingExistsGuard');
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingJsonbAggSql');
    expect(src('finance-record-sources.ts')).toContain('LOCK TABLE planning_scenarios');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
  });

  it('keeps retirement:ts-statements strictly below the r26 BEFORE=60 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(56); // r26 left 60; r27 parameterizes the journal projection family 60->58; r28 cost-allocation probe 58->57; r29 scoped EXISTS probe 57->56.
    expect(total).toBeLessThan(60);
  });
});
