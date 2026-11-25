import { describe, expect, test, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { evidenceHead } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { qualityLineageTerminalSql } from '../../../packages/server-quality/src/adapters/quality-lineage-terminal-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r39 fold, rendered by the retired
// inline `SELECT ${columns} FROM lineage${tail ? ` ${tail}` : ''}` in
// quality-lineage-terminal-sql. The four terminal heads are the byte-identical
// family folded in r24; the helper now passes the same projection, FROM relation
// and tail to evidenceHead, so rendered SQL and bind order must not change.
const terminals = [
  {
    name: 'purchase remaining-hold root',
    columns: '*',
    tail: 'ORDER BY depth DESC LIMIT 1',
    before: 'SELECT * FROM lineage ORDER BY depth DESC LIMIT 1',
    digest: '74b3d27bea8ccb1de53473865515b0e9e4d4a3f70b0843b0f7cd1d4010b9c74a',
  },
  {
    name: 'purchase roots CTE',
    columns: 'DISTINCT ON(hold_id) hold_id,root_id',
    tail: 'ORDER BY hold_id,depth DESC',
    before: 'SELECT DISTINCT ON(hold_id) hold_id,root_id FROM lineage ORDER BY hold_id,depth DESC',
    digest: '813da2a9643c5aa8f439b34ee5b8eff1e47132e8280c03a1bf7354189ea1e735',
  },
  {
    name: 'sales settlement terminal',
    columns: 'DISTINCT ON(h.id,lineage.source_company) h.*,lineage.source_company,lineage.depth source_depth',
    tail: 'JOIN inventory_quality_holds h ON h.id=lineage.id ORDER BY h.id,lineage.source_company,lineage.depth DESC',
    before: 'SELECT DISTINCT ON(h.id,lineage.source_company) h.*,lineage.source_company,lineage.depth source_depth FROM lineage JOIN inventory_quality_holds h ON h.id=lineage.id ORDER BY h.id,lineage.source_company,lineage.depth DESC',
    digest: '50d605f24eafa3caac1f9f4e4329412b48744020285057e6a7c92f7f049532fa',
  },
  {
    name: 'ecommerce settlement terminal',
    columns: 'DISTINCT id',
    tail: '',
    before: 'SELECT DISTINCT id FROM lineage',
    digest: 'd74bb9d650cd75eebc8245f7a3e897f9abd2151c30ab7524efca9bb5a2b800a3',
  },
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

describe('quality read-tail r39 lineage-terminal evidence head dedupe', () => {
  test.each(terminals)('$name: byte-identical to the pre-dedupe rendering', (t) => {
    const sql = qualityLineageTerminalSql(t.columns, t.tail);
    expect(sql).toBe(t.before);
    expect(sha(sql)).toBe(t.digest);
    expect(sql).toBe(evidenceHead(t.columns, ' FROM lineage', t.tail ? ` ${t.tail}` : ''));
    expect(sql.startsWith('SELECT ')).toBe(true);
    expect(sql.match(/SELECT /g)).toHaveLength(1);
    expect(sql).toBe(`SELECT ${t.columns} FROM lineage${t.tail ? ` ${t.tail}` : ''}`);
    expect(sql.match(/\?/g)).toBeNull();
    expect(sql.match(/\$[0-9]/g)).toBeNull();
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the helper delegates to evidenceHead and owns no inline SELECT', () => {
    const helper = read('adapters/quality-lineage-terminal-sql.ts');
    expect(helper).toContain('import { evidenceHead } from "./inspection-evidence-source-sql";');
    expect(helper).toContain("evidenceHead(columns, ' FROM lineage', tail ? ` ${tail}` : '')");
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head).toContain('export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('readers still consume the shared helper with no residual inline terminals', () => {
    const purchase = read('adapters/purchase-sources.ts');
    const sales = read('adapters/sales-package-sources.ts');
    const ecommerce = read('adapters/ecommerce-package-sources.ts');
    expect(purchase).toContain('import {qualityLineageTerminalSql} from "./quality-lineage-terminal-sql"');
    expect(sales).toContain("import {qualityLineageTerminalSql} from './quality-lineage-terminal-sql'");
    expect(ecommerce).toContain("import {qualityLineageTerminalSql} from './quality-lineage-terminal-sql'");
    for (const t of terminals) {
      expect(purchase).not.toContain(t.before);
      expect(sales).not.toContain(t.before);
      expect(ecommerce).not.toContain(t.before);
    }
  });

  test('native registry, CAS guards and all protected locks are untouched', () => {
    expect(read('adapters/native-quality-read.ts')).toContain('RD_ATTACH_HOLD_CAS');
    expect(read('adapters/native-quality-read.ts')).toContain('RD_ATTACH_SPEC_CAS');
    expect(read('adapters/native_quality_read_sql.rs')).toContain('EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=?)');
    expect(read('adapters/native_quality_read_sql.rs')).toContain('EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND company_id=? AND version=?)');
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('salesQualitySettlementSourceReadLocks');
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=23 baseline', () => {
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBeLessThan(23);
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(15);
    expect(kpi(/SELECT /g)).toBe(15);
  });
});
