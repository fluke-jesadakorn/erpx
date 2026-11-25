import { describe, expect, test, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { nativeSourceWhere } from '../../../packages/server-quality/src/adapters/native-source-wrapper-sql';
import { evidenceHead } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { ecommerceQualitySettlementSources } from '../../../packages/server-quality/src/public/ecommerce-package-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r35 fold, rendered by the retired
// inline native SELECT. The helper now passes the same projection, FROM
// relation and WHERE tail to evidenceHead; spacing and binds must not change.
const renderings = [
  ['RET_ORIGINS', "native.kind='customer-return' AND native.owner_id IN (SELECT id FROM (RETURNS) returned)",
    "SELECT native.* FROM (RET_ORIGINS) native WHERE native.kind='customer-return' AND native.owner_id IN (SELECT id FROM (RETURNS) returned)",
    '1b9e03d34653cb369a0f15c3919c777098bd693f17173b84d6ba05e845974810'],
  ['RET_HOLDS', 'native.id IN (HOLD_IDS)',
    'SELECT native.* FROM (RET_HOLDS) native WHERE native.id IN (HOLD_IDS)',
    '1884319a1fbb971c175d5634c541eb84087eda2be0286d8e6153aeff32cd45ea'],
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

describe('quality read-tail r35 native wrapper evidence head dedupe', () => {
  test.each(renderings)('source %s is byte-identical to the pre-dedupe rendering', (source, predicate, before, digest) => {
    const sql = nativeSourceWhere(source, predicate);
    expect(sql).toBe(before);
    expect(sha(sql)).toBe(digest);
    expect(sql).toBe(evidenceHead('native.*', ` FROM (${source}) native`, ` WHERE ${predicate}`));
    expect(sql.startsWith('SELECT native.* FROM (')).toBe(true);
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('both live settlement readers keep their pre-dedupe digests and carry no binds', () => {
    expect(sha(ecommerceQualitySettlementSources.returnOrigins)).toBe('6454aa7fa158402e4e8860fd87f96652547caadf9c6624f847b714ce9fac6e8a');
    expect(sha(ecommerceQualitySettlementSources.returnHoldSources)).toBe('8301ffbd29d6614d11da885da1d3b4a401c44af47f55acdccec18ca9e0aea5e1');
    for (const sql of [ecommerceQualitySettlementSources.returnOrigins, ecommerceQualitySettlementSources.returnHoldSources]) {
      expect(sql.startsWith('SELECT native.* FROM (')).toBe(true);
      expect(sql.match(/\?|\$[0-9]/g)).toBeNull();
    }
  });

  test('the helper delegates to evidenceHead and owns no inline SELECT', () => {
    const helper = read('adapters/native-source-wrapper-sql.ts');
    expect(helper).toContain('import { evidenceHead } from "./inspection-evidence-source-sql";');
    expect(helper).toContain("evidenceHead('native.*', ` FROM (${source}) native`, ` WHERE ${predicate}`)");
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head).toContain('export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('callers and all protected locks are unchanged', () => {
    const callers = read('adapters/ecommerce-package-sources.ts');
    expect(callers.match(/nativeSourceWhere\(/g)).toHaveLength(2);
    expect(callers).not.toContain('SELECT native.* FROM (');
    expect(callers).toContain("nativeSourceWhere(quality.returnOrigins,`native.kind='customer-return' AND native.owner_id ${qualityIdInSourceSql(commerce.returns, 'returned')}`)");
    expect(callers).toContain('nativeSourceWhere(quality.returnHoldSources,`native.id IN (${holdIds})`)');
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE');
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=27 baseline', () => {
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBeLessThan(27);
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(21);
    expect(kpi(/SELECT /g)).toBe(21);
  });
});
