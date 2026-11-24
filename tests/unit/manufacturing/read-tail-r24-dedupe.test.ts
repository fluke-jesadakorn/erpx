import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { manufacturingPurchaseScopedSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r24-sql';
import { manufacturingPurchaseScopedRows } from '../../../packages/server-manufacturing/src/adapters/purchases';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

/** The three purchased-scope readers (metric, workspace record, attachment)
 * inlined the same scoped-source SELECT with only the projection, alias and
 * trailing predicate varying. Originals were captured verbatim from HEAD; the
 * helper concatenates its three arguments, so whole-string byte identity of
 * each emitted SELECT is the oracle. Stand-in predicate text keeps the test
 * independent of the IAM/clock builders while still exercising the exact
 * byte boundaries the helper is responsible for. */
const S = (alias: string) => manufacturingPurchaseScopedRows(alias);

const FAMILY: readonly {
  file: string;
  args: readonly [string, string, string];
  original: string;
}[] = [
  {
    file: 'metric-purchases.ts',
    args: [
      'production_metric.id',
      'production_metric',
      `CROSS JOIN production_metric_clock WHERE production_metric.kind='order' AND M AND P`,
    ],
    original: `SELECT production_metric.id FROM ${S('production_metric')} CROSS JOIN production_metric_clock WHERE production_metric.kind='order' AND M AND P`,
  },
  {
    file: 'record-purchases.ts',
    args: [
      '1',
      'production_source',
      'CROSS JOIN production_source_clock WHERE production_source.id=? AND S',
    ],
    original: `SELECT 1 FROM ${S('production_source')} CROSS JOIN production_source_clock WHERE production_source.id=? AND S`,
  },
  {
    file: 'attachment-purchases.ts',
    args: ['1', 'production_file_scope', 'WHERE id=? AND company_id=? AND A'],
    original: `SELECT 1 FROM ${S('production_file_scope')} WHERE id=? AND company_id=? AND A`,
  },
];

describe('manufacturing read-tail r24 purchased-scope select family', () => {
  it('renders the shared scoped select byte-identically to all three originals and stays read-only', () => {
    for (const { args, original } of FAMILY) {
      expect(manufacturingPurchaseScopedSelect(...args)).toBe(original);
      expect(original).toContain('SELECT ');
      expect(original).toContain(` FROM ${S(args[1])} `);
      expect(original).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only helper, routes all three callers and leaves no residual inline SQL', () => {
    const helper = src('manufacturing-read-tail-r24-sql.ts');
    expect(helper.match(/export const manufacturingPurchaseScopedSelect/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    for (const { file } of FAMILY) {
      const caller = src(file);
      expect(caller.split('from "./manufacturing-read-tail-r24-sql"').length - 1).toBe(1);
      expect(caller.split('manufacturingPurchaseScopedSelect(').length - 1).toBe(1);
      expect(caller).not.toContain('manufacturingPurchaseScopedRows(');
    }
  });

  it('leaves the r4/r11/r15-r23 helpers, locks and FOR UPDATE witness untouched', () => {
    const fragments = src('manufacturing-read-sql-fragments.ts');
    expect(fragments).toContain('export const manufacturingRowsSelect');
    expect(fragments).toContain('export const manufacturingIdSelect');
    expect(fragments).toContain('export const manufacturingAttachmentVersionGuard');
    expect(src('manufacturing-read-tail-r15-sql.ts')).toContain('export const manufacturingOrderJoinSelect');
    expect(src('manufacturing-read-tail-r16-sql.ts')).toContain('export const manufacturingPurchaseRecordSelect');
    expect(src('manufacturing-read-tail-r17-sql.ts')).toContain('export const manufacturingSettlementFactSelect');
    expect(src('manufacturing-read-tail-r18-sql.ts')).toContain('export const manufacturingPurchaseRecordBranchSelect');
    expect(src('manufacturing-read-tail-r19-sql.ts')).toContain('export const manufacturingAccountingProfileSelect');
    expect(src('manufacturing-read-tail-r20-sql.ts')).toContain('export const manufacturingExistsGuard');
    expect(src('manufacturing-read-tail-r21-sql.ts')).toContain('export const manufacturingAttachmentImageSelect');
    expect(src('manufacturing-read-tail-r22-sql.ts')).toContain('export const manufacturingOrderDetailSelect');
    expect(src('manufacturing-read-tail-r23-sql.ts')).toContain('export const manufacturingStockFootprintSelect');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots,manufacturing_operation_events');
    expect(src('manufacturing-read-tail-r24-sql.ts')).not.toMatch(/LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE/i);
  });

  it('keeps retirement:ts-statements strictly below the r23 BEFORE=72 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(58); // r23 left 72; r24 72->70; r25 70->65; r26 EXISTS guards -> 60; r27 journal projection family 60->58.
    expect(total).toBeLessThan(60);
  });
});
