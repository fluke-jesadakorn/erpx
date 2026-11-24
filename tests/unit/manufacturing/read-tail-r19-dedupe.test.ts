import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { manufacturingAccountingProfileSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r19-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

/** The four pre-dedupe accounting-profile reader literals, captured verbatim
 * from HEAD. Whole-string byte identity is the oracle: no column, alias,
 * predicate, whitespace or bind may drift. */
const FAMILY: readonly { args: readonly [string, string, string]; original: string }[] = [
  {
    args: ["*", "", "company_id=? AND status='approved'"],
    original: "SELECT * FROM manufacturing_accounting_profiles WHERE company_id=? AND status='approved'",
  },
  {
    args: ["id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id", "", "company_id=? ORDER BY updated_at DESC"],
    original: "SELECT id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id FROM manufacturing_accounting_profiles WHERE company_id=? ORDER BY updated_at DESC",
  },
  {
    args: ["p.*,to_jsonb(p)::text source", "p", "company_id=? ORDER BY id"],
    original: "SELECT p.*,to_jsonb(p)::text source FROM manufacturing_accounting_profiles p WHERE company_id=? ORDER BY id",
  },
  {
    args: ["id,company_id,NULL::text branch_id,false advanced,to_jsonb(profile)-'status'-'version'-'updated_at'-'reviewed_by' witness", "profile", ""],
    original: "SELECT id,company_id,NULL::text branch_id,false advanced,to_jsonb(profile)-'status'-'version'-'updated_at'-'reviewed_by' witness FROM manufacturing_accounting_profiles profile",
  },
];

const CALLERS = ['accounting.ts', 'production-command-sources.ts', 'posting-profiles.ts', 'purchases.ts'] as const;

describe('manufacturing read-tail r19 accounting-profile select family', () => {
  it('renders the shared accounting-profile select byte-identically to all four originals and stays read-only', () => {
    for (const { args, original } of FAMILY) {
      expect(manufacturingAccountingProfileSelect(...args)).toBe(original);
      expect(original).toContain('SELECT ');
      expect(original).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only helper and routes every family caller through it with no residual inline SQL', () => {
    const helper = src('manufacturing-read-tail-r19-sql.ts');
    expect(helper.match(/export const manufacturingAccountingProfileSelect/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    for (const file of CALLERS) {
      const text = src(file);
      expect(text.split('manufacturingAccountingProfileSelect(').length - 1).toBe(1);
      for (const { original } of FAMILY) expect(text).not.toContain(original);
    }
  });

  it('leaves r11/r4/r15-r18 helpers and all Manufacturing source locks untouched', () => {
    const fragments = src('manufacturing-read-sql-fragments.ts');
    expect(fragments).toContain('export const manufacturingRowsSelect');
    expect(fragments).toContain('export const manufacturingIdSelect');
    expect(src('manufacturing-read-tail-r15-sql.ts')).toContain('export const manufacturingOrderJoinSelect');
    expect(src('manufacturing-read-tail-r16-sql.ts')).toContain('export const manufacturingPurchaseRecordSelect');
    expect(src('manufacturing-read-tail-r17-sql.ts')).toContain('export const manufacturingSettlementFactSelect');
    expect(src('manufacturing-read-tail-r18-sql.ts')).toContain('export const manufacturingPurchaseRecordBranchSelect');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('manufacturing-read-tail-r19-sql.ts')).not.toMatch(/LOCK\s+TABLE/i);
  });

  it('keeps retirement:ts-statements strictly below the r19 BEFORE=86 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThanOrEqual(83); // r18 lowered 89->86; r19 lowers 86->83.
    expect(total).toBeLessThan(86);
  });
});
