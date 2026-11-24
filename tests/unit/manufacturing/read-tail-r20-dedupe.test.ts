import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { manufacturingExistsGuard } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r20-sql';
import { manufacturingExistsSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-sql-fragments';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

/** The five pre-dedupe EXISTS-guard literals, captured verbatim from HEAD.
 * Whole-string byte identity is the oracle: no table, predicate, whitespace or
 * bind may drift. Three were inline October callers and two were local guard
 * constants from earlier (non-r11/r4/r15-r19) rounds. */
const FAMILY: readonly { args: readonly [string, string]; original: string }[] = [
  {
    args: ["manufacturing_accounting_profiles", "id=? AND company_id=? AND status='approved' AND version=?"],
    original: "EXISTS(SELECT 1 FROM manufacturing_accounting_profiles WHERE id=? AND company_id=? AND status='approved' AND version=?)",
  },
  {
    args: ["manufacturing_specifications", "id=? AND company_id=? AND version=? AND status='approved'"],
    original: "EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status='approved')",
  },
  {
    args: ["manufacturing_order_events", "order_id=? AND action='report' AND version=? AND actor_id<>?"],
    original: "EXISTS(SELECT 1 FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=? AND actor_id<>?)",
  },
  {
    args: ["manufacturing_orders", "id=? AND version=? AND status=?"],
    original: "EXISTS(SELECT 1 FROM manufacturing_orders WHERE id=? AND version=? AND status=?)",
  },
  {
    args: ["manufacturing_material_reservations", "id=? AND consumed_milli=0 AND released_milli=0 AND expires_at>?"],
    original: "EXISTS(SELECT 1 FROM manufacturing_material_reservations WHERE id=? AND consumed_milli=0 AND released_milli=0 AND expires_at>?)",
  },
];

/** Per-caller expected number of manufacturingExistsGuard(...) call sites. */
const CALLERS: readonly { file: string; calls: number }[] = [
  { file: 'accounting.ts', calls: 1 },
  { file: 'production-command-sources.ts', calls: 4 },
];

describe('manufacturing read-tail r20 EXISTS-guard family', () => {
  it('renders the shared guard byte-identically to all five originals and stays read-only', () => {
    for (const { args, original } of FAMILY) {
      expect(manufacturingExistsGuard(...args)).toBe(original);
      expect(original).toContain('SELECT 1 FROM ');
      expect(original).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only helper and routes every family caller through it with no residual inline SQL', () => {
    const helper = src('manufacturing-read-tail-r20-sql.ts');
    expect(helper.match(/export const manufacturingExistsGuard/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    for (const { file, calls } of CALLERS) {
      const text = src(file);
      expect(text.split('manufacturingExistsGuard(').length - 1).toBe(calls);
      for (const { original } of FAMILY) expect(text).not.toContain(original);
    }
  });

  it('routes the six EXISTS guards through one text-only probe', () => {
    expect(manufacturingExistsSelect('manufacturing_orders', 'id=?')).toBe('SELECT 1 FROM manufacturing_orders WHERE id=?');
    expect(manufacturingExistsSelect('postings', '')).toBe('SELECT 1 FROM postings');
    const probe = src('manufacturing-read-sql-fragments.ts');
    expect(probe.match(/export const manufacturingExistsSelect/g) ?? []).toHaveLength(1);
    expect(probe).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    for (const [file, needle] of [
      ['manufacturing-read-tail-r20-sql.ts', 'EXISTS(${manufacturingExistsSelect('],
      ['manufacturing-read-sql-fragments.ts', 'EXISTS(${manufacturingExistsSelect('],
      ['posting-setup-sources.ts', 'EXISTS(${manufacturingExistsSelect(table,predicate)})'],
      ['initial-physical-stock-sources.ts', 'EXISTS(${manufacturingExistsSelect('],
      ['production-command-sources.ts', 'NOT EXISTS(${manufacturingExistsSelect('],
    ] as const) expect(src(file)).toContain(needle);
    // The r26 exclusions stay inline and untouched.
    expect(src('purchases.ts')).toContain('export const commandReceiptSelect');
    expect(probe).toContain('export const manufacturingNativeProductionProbe');
    expect(src('attachment-purchases.ts')).toContain('function sourcePredicate(');
  });

  it('leaves r11/r4/r15-r19 helpers and every Manufacturing source lock untouched', () => {
    const fragments = src('manufacturing-read-sql-fragments.ts');
    expect(fragments).toContain('export const manufacturingRowsSelect');
    expect(fragments).toContain('export const manufacturingIdSelect');
    expect(src('manufacturing-read-tail-r15-sql.ts')).toContain('export const manufacturingOrderJoinSelect');
    expect(src('manufacturing-read-tail-r16-sql.ts')).toContain('export const manufacturingPurchaseRecordSelect');
    expect(src('manufacturing-read-tail-r17-sql.ts')).toContain('export const manufacturingSettlementFactSelect');
    expect(src('manufacturing-read-tail-r18-sql.ts')).toContain('export const manufacturingPurchaseRecordBranchSelect');
    expect(src('manufacturing-read-tail-r19-sql.ts')).toContain('export const manufacturingAccountingProfileSelect');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
    expect(src('finance-record-sources.ts')).toContain('LOCK TABLE planning_scenarios');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('attachment-read.ts')).toContain('WHERE ${currentScope.where}');
    expect(src('manufacturing-read-tail-r20-sql.ts')).not.toMatch(/LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE/i);
  });

  it('keeps retirement:ts-statements strictly below the r19 BEFORE=83 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThanOrEqual(79); // r19 lowered 86->83; r20 lowers 83->79 (five inline guards -> one helper).
    expect(total).toBeLessThan(83);
  });
});
