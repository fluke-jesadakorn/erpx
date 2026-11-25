import { describe, expect, test, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { evidenceHead } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { qualitySourceRows } from '../../../packages/server-quality/src/adapters/quality-source-rows-sql';
import { salesQualitySettlementSources } from '../../../packages/server-quality/src/adapters/sales-package-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r37 fold, rendered by the retired
// inline `SELECT * FROM ${table}` in quality-source-rows-sql. The helper now
// passes the same '*' projection, FROM relation and empty tail to evidenceHead;
// rendered SQL and bind order must not change.
const renderings = [
  ['holds', 'inventory_quality_holds',
    'SELECT * FROM inventory_quality_holds',
    '59a99c9d8dde215add6f60795d76960e3bcc88825213e5968b39524a7a1db73c'],
  ['inspections', 'inventory_quality_inspections',
    'SELECT * FROM inventory_quality_inspections',
    '0ae9cba2edae3f78b8ae6fbf57c30ece6b43f59f89602279681c8ae79296868a'],
  ['dispositions', 'inventory_quality_disposition_events',
    'SELECT * FROM inventory_quality_disposition_events',
    'c5193ddd4e487e5cd3ba43d00f0a5bf6154fac5fd98d009a59bea6d625648de1'],
  ['cancellations', 'inventory_quality_cancellations',
    'SELECT * FROM inventory_quality_cancellations',
    '9585215bb7b0a6fdb4d885448aab3a56ed35499a7cea0ce7d631d713823ac3c4'],
] as const;

const kpi = (pattern: RegExp) => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) {
        total += (readFileSync(path, 'utf8').match(pattern) ?? []).length;
      }
    }
  };
  walk(srcRoot);
  return total;
};

describe('quality read-tail r37 source-rows evidence head dedupe', () => {
  test.each(renderings)('source $0 is byte-identical to the pre-dedupe rendering', (_name, table, before, digest) => {
    const sql = qualitySourceRows(table);
    expect(sql).toBe(before);
    expect(sha(sql)).toBe(digest);
    expect(sql).toBe(evidenceHead('*', ` FROM ${table}`, ''));
    expect(sql.startsWith('SELECT * FROM ')).toBe(true);
    expect(sql.match(/\?|\$[0-9]/g)).toBeNull();
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the bound purchase hold read keeps its exact pre-dedupe SQL and bind order', () => {
    const sql = `${qualitySourceRows('inventory_quality_holds')} WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)`;
    expect(sha(sql)).toBe('6101e5c7e1152956bb540e3cff88b2212fb83d51c4518b564600ed5ebe070ac5');
    expect(sql).toBe('SELECT * FROM inventory_quality_holds WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)');
    expect(sql.match(/\?/g)).toHaveLength(4);
    expect(read('adapters/purchase-sources.ts')).toContain(').bind(id,scope.companyId,scope.branchId,scope.branchId).first<QualitySource>()');
  });

  test('every live settlement source still renders its captured digest', () => {
    const expected: Record<string, string> = {
      holds: renderings[0][3],
      inspections: renderings[1][3],
      dispositions: renderings[2][3],
      cancellations: renderings[3][3],
    };
    for (const [key, digest] of Object.entries(expected)) {
      const sql = salesQualitySettlementSources[key as keyof typeof salesQualitySettlementSources];
      expect(typeof sql).toBe('string');
      expect(sha(sql as string)).toBe(digest);
      expect(sql).toBe(`SELECT * FROM ${renderings.find((r) => r[3] === digest)![1]}`);
    }
  });

  test('the helper delegates to evidenceHead and owns no inline SELECT', () => {
    const helper = read('adapters/quality-source-rows-sql.ts');
    expect(helper).toContain('import { evidenceHead } from "./inspection-evidence-source-sql";');
    expect(helper).toContain('evidenceHead("*", ` FROM ${table}`, "")');
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head).toContain('export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('callers and all protected locks are unchanged', () => {
    const purchase = read('adapters/purchase-sources.ts');
    const sales = read('adapters/sales-package-sources.ts');
    expect(purchase.match(/qualitySourceRows\(/g)).toHaveLength(1);
    expect(sales.match(/qualitySourceRows\(/g)).toHaveLength(4);
    expect(purchase).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_disposition_events,inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE MODE');
    expect(sales).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(purchase).toContain('qualityPurchaseSourceReadLocks');
    expect(sales).toContain('salesQualitySettlementSourceReadLocks');
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=25 baseline', () => {
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBeLessThan(25);
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(23);
    expect(kpi(/SELECT /g)).toBe(23);
  });
});
