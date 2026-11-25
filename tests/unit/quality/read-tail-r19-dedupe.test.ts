import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityAttachmentSourceSelect } from '../../../packages/server-quality/src/adapters/quality-attachment-source-select-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Pre-dedupe attachment source projection literals copied byte-for-byte from HEAD.
const rows = '${qualityPurchaseSourceRows}';
const families = [
  ['branch_id', 'kind', 'witness::text image', `(${rows}) actual_quality_source`,
    `SELECT id,company_id,branch_id,NULL::text department_id,kind,witness::text image FROM (${rows}) actual_quality_source WHERE id=? AND company_id=?`],
  ['NULL::text branch_id', "'advanced' kind", "(to_jsonb(s)-'version'-'updated_at')::text image", 'inventory_quality_specs s',
    "SELECT id,company_id,NULL::text branch_id,NULL::text department_id,'advanced' kind,(to_jsonb(s)-'version'-'updated_at')::text image FROM inventory_quality_specs s WHERE id=? AND company_id=?"],
] as const;

describe('quality attachment source projection family dedupe (r19)', () => {
  test.each(families)('helper reproduces the pre-dedupe projection byte-for-byte', (branch, kind, image, from, before) => {
    const sql = qualityAttachmentSourceSelect(branch, kind, image, from);
    expect(sql).toBe(before);
    expect(sql.match(/SELECT /g)).toHaveLength(1);
    expect((sql.match(/\?/g) ?? []).length).toBe(2);
  });
  test('the hold and spec readers share the helper with no inline projections', () => {
    const text = read('adapters/attachment-purchases.ts');
    expect(text.match(/qualityAttachmentSourceSelect\(/g)).toHaveLength(2);
    for (const [, , , , before] of families) expect(text).not.toContain(before);
    expect(text).toContain('import {qualityAttachmentSourceSelect} from "./quality-attachment-source-select-sql"');
    expect(text).toContain('qualityAttachmentSourceSelect("branch_id","kind","witness::text image",');
    expect(text).toContain('qualityAttachmentSourceSelect("NULL::text branch_id",');
    expect(read('adapters/quality-attachment-source-select-sql.ts').match(/SELECT /g) ?? []).toHaveLength(0);
  });
  test('the helper is read-only; descriptor cutover, locks and r1-r18 helpers stay untouched', () => {
    const helper = read('adapters/quality-attachment-source-select-sql.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations');
    const counts: Record<string, number> = {
      'quality-work-scope-sql': 1, 'native-source-wrapper-sql': 0, 'inspection-evidence-source-sql': 1,
      'quality-lineage-recursion-sql': 1, 'quality-child-agg-sql': 1,
      'quality-receipt-guard-sql': 1, 'quality-source-rows-sql': 0, 'quality-scope-sql': 1,
      'quality-alert-recipient-sql': 0, 'quality-attachment-scope-read-sql': 1, 'quality-id-source-sql': 1,
    };
    for (const [name, count] of Object.entries(counts)) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g) ?? []).toHaveLength(count);
    }
  });
});

describe('quality read-tail r19 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=73 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(73);
    expect(total).toBe(24);
  });
});
