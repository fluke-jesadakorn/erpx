import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import { manufacturingMetricBreakdownSelect } from '../../../modules/manufacturing/server/src/adapters/manufacturing-read-tail-r32-sql';

const srcRoot = resolve(__dirname, '../../../modules/manufacturing/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

/** The completed-orders count and production-cost sum projections opened the
 * same byte-identical label/value head, with only the aggregation expression and
 * the FROM/WHERE/GROUP tail varying. Originals were captured verbatim from HEAD;
 * one parameterized emitter now renders both byte for byte on each caller's own
 * prepared statement. */
describe('manufacturing read-tail r32 metric breakdown head family', () => {
  it('renders both former projection heads byte-identically through one emitter', () => {
    expect(manufacturingMetricBreakdownSelect('branch_label', 'count(*)::text', ' FROM orders o GROUP BY 1 ORDER BY 1'))
      .toBe('SELECT branch_label label,count(*)::text value FROM orders o GROUP BY 1 ORDER BY 1');
    expect(manufacturingMetricBreakdownSelect('p.name', 'COALESCE(sum(m.amount_minor::numeric),0)::text', `
      FROM movements m
      GROUP BY 1,p.id ORDER BY 1`))
      .toBe(`SELECT p.name label,COALESCE(sum(m.amount_minor::numeric),0)::text value
      FROM movements m
      GROUP BY 1,p.id ORDER BY 1`);
    for (const sql of [
      manufacturingMetricBreakdownSelect('label', 'count(*)::text', ' FROM t'),
      manufacturingMetricBreakdownSelect('label', 'sum(x)::text', '\n FROM t'),
    ]) {
      expect(sql.startsWith('SELECT ')).toBe(true);
      expect(sql).toContain(' label,');
      expect(sql).toContain(' value');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only emitter and routes both callers through it', () => {
    const helper = src('manufacturing-read-tail-r32-sql.ts');
    expect(helper.match(/export const manufacturingMetricBreakdownSelect =/g) ?? []).toHaveLength(1);
    expect(helper.match(/SELECT/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    const projections = src('executive-metric-projections.ts');
    expect(projections.split('manufacturingMetricBreakdownSelect(').length - 1).toBe(2); // two call sites
    expect(projections).not.toContain('`SELECT ${label} label,count(*)::text value');
    expect(projections).not.toContain('`SELECT ${label} label,COALESCE(sum(m.amount_minor::numeric),0)::text value');
  });

  it('leaves the r4/r11/r15-r31 helpers, locks and branch-label fragment untouched', () => {
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingExistsSelect');
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingTargetDescriptorSelect');
    expect(src('manufacturing-read-tail-r29-sql.ts')).toContain('export const manufacturingScopedExistsProbe');
    expect(src('manufacturing-read-tail-r30-sql.ts')).toContain('export const manufacturingScopeCte');
    expect(src('manufacturing-read-tail-r31-sql.ts')).toContain('export const manufacturingAllowedSelect');
    expect(src('package-obligations.ts')).toContain('export const manufacturingCostAllocationProbe');
    expect(src('executive-metric-projections.ts')).toContain('const branchDisplayLabel =');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
  });

  it('keeps retirement:ts-statements strictly below the r31 BEFORE=53 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(42); // r31 left 53; r32 parameterizes the metric breakdown head family 53->52; r34 r15/r23 head fold 52->51; r35 scope-row head fold 51->50; r36 owner-table head fold 50->49; r37 projection-from-source head fold 49->48; r22 native detail-read cut 46->45; package-obligation owner-table heads 45->42.
    expect(total).toBeLessThan(53);
  });
});
