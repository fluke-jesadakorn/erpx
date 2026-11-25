import { describe, expect, test, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { evidenceHead } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { qualityIdInSourceSql } from '../../../packages/server-quality/src/adapters/quality-id-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const writer = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i;

// Byte oracles captured at HEAD before the r41 fold, rendered by the retired
// inline `IN (SELECT id FROM (${source}) ${alias})` in qualityIdInSourceSql and
// consumed by the Ecommerce customer-return settlement reads. The head now
// routes through evidenceHead, so every rendering, bind order and spacing must
// stay byte-identical to these digests. `head` is the exact
// evidenceHead(projection, from, tail) triple each rendering must equal.
type Arm = {
  name: string;
  source: string;
  alias: string;
  head: [string, string, string];
  before: string;
  digest: string;
};
const arms: Arm[] = [
  {
    name: 'customer-return origins membership',
    source: 'returns',
    alias: 'returned',
    head: ['id', ' FROM (returns) returned', ''],
    before: 'IN (SELECT id FROM (returns) returned)',
    digest: '9c65d2386937c0c84889829f705b5eec0e19b0783e7f3637345c398f4698a4f1',
  },
  {
    name: 'customer-return line membership',
    source: 'returnLines',
    alias: 'line',
    head: ['id', ' FROM (returnLines) line', ''],
    before: 'IN (SELECT id FROM (returnLines) line)',
    digest: '9af7c884c59aa8bc3aa60678ee5a03ba22c11bec4932163ed468327396a949d8',
  },
  {
    name: 'customer-return lot membership',
    source: 'returnLots',
    alias: 'lot',
    head: ['id', ' FROM (returnLots) lot', ''],
    before: 'IN (SELECT id FROM (returnLots) lot)',
    digest: '5346423b86db353959051d7b29e64e969e543bb5521fbb4694a38a20c9ab87fe',
  },
  {
    name: 'generic sourced membership with alias',
    source: 'commerce_returns r',
    alias: 'settled',
    head: ['id', ' FROM (commerce_returns r) settled', ''],
    before: 'IN (SELECT id FROM (commerce_returns r) settled)',
    digest: '4fd9e293f218574c7d763e497510eede7bb401b63e8a5782aed3b5c5135e11e2',
  },
];

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

describe('quality read-tail r41 id-source membership evidence head dedupe', () => {
  test.each(arms)('$name: byte-identical to the pre-dedupe rendering', (t) => {
    const sql = qualityIdInSourceSql(t.source, t.alias);
    expect(sql).toBe(t.before);
    expect(sha(sql)).toBe(t.digest);
    expect(sql).toBe(`IN (${evidenceHead(...t.head)})`);
    expect(sql.startsWith('IN (SELECT ')).toBe(true);
    expect(sql.match(/SELECT /g)).toHaveLength(1);
    expect((sql.match(/\?/g) ?? []).length).toBe(0);
    expect(sql.match(/\$[0-9]/g)).toBeNull();
    expect(sql).not.toMatch(writer);
  });

  test('the helper delegates to evidenceHead and owns no inline literal', () => {
    const helper = read('adapters/quality-id-source-sql.ts');
    expect(helper).toContain('import { evidenceHead } from "./inspection-evidence-source-sql";');
    expect(helper).toContain('evidenceHead("id", ` FROM (${source}) ${alias}`, "")');
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head).toContain('export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(helper).not.toMatch(writer);
  });

  test('the consumer still uses the shared head with no residual inline literal', () => {
    const consumer = read('adapters/ecommerce-package-sources.ts');
    expect(consumer).toContain("import {qualityIdInSourceSql} from './quality-id-source-sql';");
    expect(consumer).toContain("qualityIdInSourceSql(commerce.returns, 'returned')");
    expect(consumer).toContain("qualityIdInSourceSql(commerce.returnLines, 'line')");
    expect(consumer).toContain("qualityIdInSourceSql(commerce.returnLots, 'lot')");
    for (const t of arms) expect(read('adapters/quality-id-source-sql.ts')).not.toContain(t.before);
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

  test('retirement:ts-statements is strictly lower than the BEFORE=21 baseline', () => {
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBeLessThan(2);
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(1);
    expect(kpi(/SELECT /g)).toBe(1);
  });
});

describe('quality final canonical head audit', () => {
  test('the sole SELECT literal is the parameterized owner evidenceHead', () => {
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head).toContain('export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;');
    expect((head.match(/SELECT /g) ?? [])).toHaveLength(1);
    expect(kpi(/SELECT /g)).toBe(1);
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(1);
  });

  test('the head is irreducible: many synchronous builders delegate, none owns a SELECT', () => {
    const consumers = ['adapters/alerts.ts', 'adapters/attachment-read-sources.ts', 'adapters/calendar-quality-facts.ts',
      'adapters/executive-main-evidence-sources.ts', 'adapters/executive-metric-projections.ts',
      'adapters/native-source-wrapper-sql.ts', 'adapters/product-entry.ts', 'adapters/purchases.ts',
      'adapters/purchase-sources.ts', 'adapters/quality-alert-recipient-sql.ts',
      'adapters/quality-attachment-source-select-sql.ts',
      'adapters/quality-lineage-recursion-sql.ts', 'adapters/quality-lineage-terminal-sql.ts',
      'adapters/quality-receipt-guard-sql.ts', 'adapters/quality-scope-sql.ts',
      'adapters/quality-source-arm-head.ts', 'adapters/quality-source-rows-sql.ts',
      'adapters/record-purchases.ts', 'adapters/supply-package-obligations.ts',
      'adapters/quality-id-source-sql.ts', 'adapters/attachment-read.ts'];
    expect(consumers.length).toBeGreaterThanOrEqual(20);
    expect(read('adapters/quality-child-agg-sql.ts')).toContain('scopeSql(');
    expect(read('adapters/quality-scope-sql.ts')).toContain('evidenceHead(columns');
    for (const path of consumers) {
      expect(read(path)).toContain('evidenceHead(');
      expect(read(path).match(/SELECT /g) ?? []).toHaveLength(0);
    }
    // Static exported SQL strings depend on synchronous head expansion.
    expect(read('adapters/purchase-sources.ts')).toContain('export const qualityPurchaseSourceRows=');
    expect(read('adapters/quality-source-arm-head.ts')).toContain('export const qualitySourceArmHead =');
  });

  test('the native read-plan registry is an allow-list of fixed statements, not a head template', () => {
    const registry = read('adapters/native_quality_read_sql.rs');
    expect(registry).toContain('pub fn read_statement(kind: &str) -> Option<&\'static QualityReadStatement>');
    expect(registry).not.toContain('RD_EVIDENCE_HEAD');
    expect(registry).not.toContain('SELECT ${projection}');
    expect(registry).not.toContain('caller_sql');
    const http = read('adapters/quality_http.rs');
    expect(http).toContain('native_quality_read_sql::read_statement(kind)');
    expect(http).toContain('values.len() != plan.bind_count');
    // A head taking arbitrary projection/FROM/tail fragments is not an
    // allow-listed kind, and native transport is async HTTP, so it cannot
    // replace a synchronous template expanded inside exported SQL strings.
  });
});
