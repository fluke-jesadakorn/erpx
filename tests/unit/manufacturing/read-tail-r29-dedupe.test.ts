import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import { manufacturingScopedExistsProbe } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r29-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

const PURCHASED = 'EXISTS(SELECT 1 FROM (SUBQ_A) purchased_production WHERE purchased_production.id=o.id)';
const POSTING = 'EXISTS(SELECT 1 FROM (SUBQ_B) actual WHERE actual.posting_id=posting.id AND actual.company_id=posting.company_id AND actual.id=posting.journal_id)';

/** The cost-evidence and existing-posting readers inlined the same
 * `EXISTS(SELECT 1 FROM (<source rows>) <alias> WHERE <predicate>)` head with
 * only the source rows, alias and predicate varying and no binds of their own.
 * Originals were captured verbatim from HEAD; one parameterized emitter now
 * renders both byte for byte on the caller's own statement. This suite pins
 * that identity, the single definition, the untouched lock/helper set and the
 * lower read KPI. */
describe('manufacturing read-tail r29 scoped EXISTS probe family', () => {
  it('emits both former probes byte-identically through one parameterized emitter', () => {
    expect(manufacturingScopedExistsProbe('SUBQ_A', 'purchased_production', 'purchased_production.id=o.id')).toBe(PURCHASED);
    expect(manufacturingScopedExistsProbe('SUBQ_B', 'actual', 'actual.posting_id=posting.id AND actual.company_id=posting.company_id AND actual.id=posting.journal_id')).toBe(POSTING);
    expect(PURCHASED.startsWith('EXISTS(SELECT 1 FROM (')).toBe(true);
    expect(POSTING.startsWith('EXISTS(SELECT 1 FROM (')).toBe(true);
    for (const sql of [PURCHASED, POSTING]) {
      expect(sql).toContain('EXISTS(SELECT 1 FROM (');
      expect(sql).toContain(' WHERE ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only emitter and routes both probes through it', () => {
    const helper = src('manufacturing-read-tail-r29-sql.ts');
    expect(helper.match(/export const manufacturingScopedExistsProbe =/g) ?? []).toHaveLength(1);
    expect(helper.match(/SELECT/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    const evidence = src('executive-main-evidence-sources.ts');
    const posting = src('existing-posting-sources.ts');
    expect(evidence.split('manufacturingScopedExistsProbe(').length - 1).toBe(1); // one call site
    expect(posting.split('manufacturingScopedExistsProbe(').length - 1).toBe(1);
    expect(evidence).not.toContain('EXISTS(SELECT 1 FROM (${purchased.sql}) purchased_production WHERE purchased_production.id=o.id)');
    expect(posting).not.toContain('EXISTS(SELECT 1 FROM (${manufacturingFinanceRecordSources.posting}) actual WHERE');
  });

  it('leaves the r4/r11/r15-r28 helpers, locks and FOR UPDATE witness untouched', () => {
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingExistsSelect');
    expect(src('package-obligations.ts')).toContain('export const manufacturingCostAllocationProbe');
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingJsonbAggSql');
    expect(src('manufacturing-read-tail-r20-sql.ts')).toContain('export const manufacturingExistsGuard');
    expect(src('finance-record-sources.ts')).toContain('const manufacturingJournalSelect=');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
  });

  it('keeps retirement:ts-statements strictly below the r28 BEFORE=57 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(49); // r28 left 57; r29 parameterizes the scoped EXISTS probe family 57->56; r30 scope CTE family 56->55; r31 allowed-select head family 55->53; r32 metric breakdown head 53->52; r34 r15/r23 head fold 52->51; r35 scope-row head fold 51->50; r36 owner-table head fold 50->49.
    expect(total).toBeLessThan(57);
  });
});
