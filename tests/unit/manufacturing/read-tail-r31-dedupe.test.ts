import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import { manufacturingAllowedSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r31-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

/** The attachment-purchase file-scope witness and the two purchased-command
 * assignment guards opened the same `SELECT <predicate> allowed` head, with only
 * the predicate expression and the optional FROM tail varying. Originals were
 * captured verbatim from HEAD; one parameterized emitter now renders all three
 * byte for byte on each caller's own prepared statement. */
describe('manufacturing read-tail r31 allowed-select head family', () => {
  const predicate = 'EXISTS(SELECT 1 FROM source s WHERE s.id=?)';
  const scopeRows = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';

  it('renders the three former guard heads byte-identically through one emitter', () => {
    expect(manufacturingAllowedSelect(predicate)).toBe(`SELECT ${predicate} allowed`);
    expect(manufacturingAllowedSelect('assignment_predicate()', `FROM ${scopeRows} production_scope`))
      .toBe('SELECT assignment_predicate() allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) production_scope');
    expect(manufacturingAllowedSelect('(jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array(?::text))', 'FROM assignments WHERE id=?'))
      .toBe('SELECT (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array(?::text)) allowed FROM assignments WHERE id=?');
    for (const sql of [manufacturingAllowedSelect(predicate), manufacturingAllowedSelect(predicate, 'FROM assignments WHERE id=?')]) {
      expect(sql.startsWith('SELECT ')).toBe(true);
      expect(sql).toContain(' allowed');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only emitter and routes all three callers through it', () => {
    const helper = src('manufacturing-read-tail-r31-sql.ts');
    expect(helper.match(/export const manufacturingAllowedSelect =/g) ?? []).toHaveLength(1);
    expect(helper.match(/SELECT/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    const attachment = src('attachment-purchases.ts');
    const purchases = src('purchases.ts');
    expect(attachment.split('manufacturingAllowedSelect(').length - 1).toBe(1); // one call site
    expect(purchases.split('manufacturingAllowedSelect(').length - 1).toBe(2); // two call sites
    expect(attachment).not.toContain('`SELECT ${sourcePredicate(');
    expect(purchases).not.toContain('`SELECT ${assignmentReadSql(');
    expect(purchases).not.toContain('"SELECT (jsonb_array_length(');
  });

  it('leaves the r4/r11/r15-r30 helpers, locks and FOR UPDATE witness untouched', () => {
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingExistsSelect');
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingScopedCte');
    expect(src('manufacturing-read-tail-r29-sql.ts')).toContain('export const manufacturingScopedExistsProbe');
    expect(src('manufacturing-read-tail-r30-sql.ts')).toContain('export const manufacturingScopeCte');
    expect(src('package-obligations.ts')).toContain('export const manufacturingCostAllocationProbe');
    expect(src('finance-record-sources.ts')).toContain('const manufacturingJournalSelect=');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
  });

  it('keeps retirement:ts-statements strictly below the r30 BEFORE=55 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(51); // r30 left 55; r31 parameterizes the allowed-select head family 55->53; r32 metric breakdown head 53->52; r34 r15/r23 head fold 52->51.
    expect(total).toBeLessThan(55);
  });
});
