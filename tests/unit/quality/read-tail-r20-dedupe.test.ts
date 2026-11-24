import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualitySourceUnionSelect } from '../../../packages/server-quality/src/adapters/quality-source-union-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Pre-dedupe UNION ALL arms copied byte-for-byte from HEAD (r19 source).
const rows = '(${qualityPurchaseSourceRows})';
const families = [
  ['s.id', 's.company_id', 's.branch_id', 'NULL::text department_id', 's.kind', `${rows} s`,
    `SELECT s.id,s.company_id,s.branch_id,NULL::text department_id,s.kind FROM ${rows} s`],
  ['a.id', 's.company_id', 's.branch_id', 'NULL::text', 's.kind', `inventory_quality_alerts a JOIN ${rows} s ON s.id=a.hold_id`,
    `SELECT a.id,s.company_id,s.branch_id,NULL::text,s.kind FROM inventory_quality_alerts a JOIN ${rows} s ON s.id=a.hold_id`],
  ['id', 'company_id', 'NULL::text', 'NULL::text', "'advanced'", 'inventory_quality_specs',
    "SELECT id,company_id,NULL::text,NULL::text,'advanced' FROM inventory_quality_specs"],
] as const;

describe('quality source projection UNION family dedupe (r20)', () => {
  test.each(families)('helper reproduces the pre-dedupe projection byte-for-byte', (id, company, branch, department, kind, from, before) => {
    const sql = qualitySourceUnionSelect(id, company, branch, department, kind, from);
    expect(sql).toBe(before);
    expect(sql.match(/SELECT /g)).toHaveLength(1);
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test('byte-for-byte recomposition of the three-arm UNION ALL', () => {
    const native = families.map(([id, company, branch, department, kind, from]) =>
      qualitySourceUnionSelect(id, company, branch, department, kind, from)).join(' UNION ALL ');
    expect(native).toBe(
      `SELECT s.id,s.company_id,s.branch_id,NULL::text department_id,s.kind FROM ${rows} s UNION ALL ` +
      `SELECT a.id,s.company_id,s.branch_id,NULL::text,s.kind FROM inventory_quality_alerts a JOIN ${rows} s ON s.id=a.hold_id UNION ALL ` +
      `SELECT id,company_id,NULL::text,NULL::text,'advanced' FROM inventory_quality_specs`);
  });
  test('the workspace source reader shares the helper with no inline projections', () => {
    const text = read('adapters/record-purchases.ts');
    expect(text.match(/qualitySourceUnionSelect\(/g)).toHaveLength(3);
    for (const [, , , , , , before] of families) expect(text).not.toContain(before);
    expect(text).toContain('import {qualitySourceUnionSelect} from "./quality-source-union-sql"');
    expect(read('adapters/quality-source-union-sql.ts').match(/SELECT /g)).toHaveLength(1);
  });
  test('the helper is read-only; locks, descriptor cutover and r1-r19 helpers stay untouched', () => {
    const helper = read('adapters/quality-source-union-sql.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs,inventory_quality_spec_metrics,inventory_quality_inspections,inventory_quality_measurements,inventory_quality_inspection_attachments,inventory_quality_cancellations,inventory_quality_disposition_events,inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE ROW EXCLUSIVE MODE');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE');
    expect(read('adapters/attachments-policy-facts.ts')).toContain('descriptorTargets');
    expect(read('adapters/attachments-service-facts.ts')).toContain('UPDATE ${descriptor.table} SET version=version+1');
    const counts: Record<string, number> = {
      'quality-work-scope-sql': 1, 'native-source-wrapper-sql': 1, 'inspection-evidence-source-sql': 1,
      'quality-record-source-sql': 1, 'quality-lineage-recursion-sql': 2, 'quality-child-agg-sql': 1,
      'quality-receipt-guard-sql': 1, 'quality-source-rows-sql': 1, 'quality-scope-sql': 2,
      'quality-alert-recipient-sql': 1, 'quality-attachment-scope-read-sql': 1, 'quality-id-source-sql': 1,
      'quality-attachment-source-select-sql': 1, 'quality-source-union-sql': 1,
    };
    for (const [name, count] of Object.entries(counts)) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g)).toHaveLength(count);
    }
  });
});

describe('quality read-tail r20 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=72 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(72);
    expect(total).toBe(59);
  });
});
