import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import { manufacturingScopeCte } from '../../../modules/manufacturing/server/src/adapters/manufacturing-read-tail-r30-sql';

const srcRoot = resolve(__dirname, '../../../modules/manufacturing/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

const FULL = 'WITH scope AS(SELECT ?::text company_id,?::boolean full)';
const PLAIN = 'WITH scope AS(SELECT ?::text company_id)';

/** The package-obligation facts query and the posting source-state query opened
 * the same scope head with only the optional boolean `full` column varying.
 * Originals were captured verbatim from HEAD; one parameterized emitter now
 * renders both byte for byte on the caller's own statement. This suite pins
 * that identity, the single definition, the untouched lock/helper set and the
 * lower read KPI. */
describe('manufacturing read-tail r30 scope CTE family', () => {
  it('emits both former scope heads byte-identically through one parameterized emitter', () => {
    expect(manufacturingScopeCte(true)).toBe(FULL);
    expect(manufacturingScopeCte(false)).toBe(PLAIN);
    expect(FULL.startsWith(PLAIN.slice(0, PLAIN.length - 1))).toBe(true);
    for (const sql of [FULL, PLAIN]) {
      expect(sql).toContain('WITH scope AS(');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only emitter and routes both callers through it', () => {
    const helper = src('manufacturing-read-tail-r30-sql.ts');
    expect(helper.match(/export const manufacturingScopeCte =/g) ?? []).toHaveLength(1);
    expect(helper.match(/SELECT/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    const pkg = src('package-obligations.ts');
    const posting = src('posting-setup-sources.ts');
    expect(pkg.split('manufacturingScopeCte(').length - 1).toBe(1); // one call site
    expect(posting.split('manufacturingScopeCte(').length - 1).toBe(1);
    expect(pkg).not.toContain('WITH scope AS(SELECT ?::text company_id,?::boolean full)');
    expect(posting).not.toContain('WITH scope AS(SELECT ?::text company_id)');
  });

  it('leaves the r4/r11/r15-r29 helpers, locks and FOR UPDATE witness untouched', () => {
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingExistsSelect');
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingScopedCte');
    expect(src('manufacturing-read-tail-r29-sql.ts')).toContain('export const manufacturingScopedExistsProbe');
    expect(src('package-obligations.ts')).toContain('export const manufacturingCostAllocationProbe');
    expect(src('finance-record-sources.ts')).toContain('const manufacturingJournalSelect=');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
  });

  it('keeps retirement:ts-statements strictly below the r29 BEFORE=56 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(42); // r29 left 56; r30 parameterizes the scope CTE family 56->55; r31 allowed-select head family 55->53; r32 metric breakdown head 53->52; r34 r15/r23 head fold 52->51; r35 scope-row head fold 51->50; r36 owner-table head fold 50->49; r37 projection-from-source head fold 49->48; r22 native detail-read cut 46->45; package-obligation owner-table heads 45->42.
    expect(total).toBeLessThan(56);
  });
});
