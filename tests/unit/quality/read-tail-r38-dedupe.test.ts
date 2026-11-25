import { describe, expect, test, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { evidenceHead } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { qualityAttachmentScopeReadSql } from '../../../packages/server-quality/src/adapters/quality-attachment-scope-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r38 fold, rendered by the retired
// inline `SELECT company_id,${columns} FROM ${table} WHERE ${predicate}` in
// quality-attachment-scope-read-sql. The helper now passes the same projection,
// FROM relation and WHERE predicate to evidenceHead; rendered SQL and bind
// order must not change.
const renderings = [
  {
    variant: 'legacy_hold',
    projection: 'company_id,branch_id branch_id,NULL department_id,version',
    table: 'inventory_quality_holds',
    predicate: 'id=?',
    before: 'SELECT company_id,branch_id branch_id,NULL department_id,version FROM inventory_quality_holds WHERE id=?',
    digest: 'ee7c2b95215d5edfc4972070826b41aa4504e2c21909aeec7b84d171e15ca3a3',
    binds: 1,
  },
  {
    variant: 'hold',
    projection: 'company_id,branch_id AS branch_id,NULL AS department_id,version AS version',
    table: 'inventory_quality_holds',
    predicate: 'id=? AND company_id=?',
    before: 'SELECT company_id,branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_quality_holds WHERE id=? AND company_id=?',
    digest: '798bb6c233bc9f8757e3b69f3359bff54d05448e22c8d7886c2975ff6b2f2091',
    binds: 2,
  },
  {
    variant: 'spec',
    projection: 'company_id,NULL::text branch_id,NULL::text department_id,version',
    table: 'inventory_quality_specs',
    predicate: 'id=? AND company_id=?',
    before: 'SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM inventory_quality_specs WHERE id=? AND company_id=?',
    digest: '5029118db5f6ea2182591d4efe6d842dd0d760953122e294397cd49a531f3b5f',
    binds: 2,
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

describe('quality read-tail r38 attachment scope-read evidence head dedupe', () => {
  test.each(renderings)('variant $variant is byte-identical to the pre-dedupe rendering', (r) => {
    const sql = qualityAttachmentScopeReadSql(r.variant);
    expect(sql).toBe(r.before);
    expect(sha(sql)).toBe(r.digest);
    expect(sql).toBe(evidenceHead(r.projection, ` FROM ${r.table}`, ` WHERE ${r.predicate}`));
    expect(sql.startsWith(`SELECT company_id,`)).toBe(true);
    expect(sql).toBe(`SELECT ${r.projection} FROM ${r.table} WHERE ${r.predicate}`);
    expect(sql.match(/\?/g)).toHaveLength(r.binds);
    expect(sql.match(/\$[0-9]/g)).toBeNull();
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the helper delegates to evidenceHead and owns no inline SELECT', () => {
    const helper = read('adapters/quality-attachment-scope-read-sql.ts');
    expect(helper).toContain('import { evidenceHead } from "./inspection-evidence-source-sql";');
    expect(helper).toContain('evidenceHead(`company_id,${columns}`, ` FROM ${table}`, ` WHERE ${predicate}`)');
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head).toContain('export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('callers and their [targetId, companyId] bind order are unchanged', () => {
    const migration = read('adapters/attachment-migration-owner.ts');
    const policy = read('adapters/attachments-policy-facts.ts');
    const operational = read('adapters/attachments-operational-facts.ts');
    expect(migration.match(/qualityAttachmentScopeReadSql\(/g)).toHaveLength(1);
    expect(policy.match(/qualityAttachmentScopeReadSql\(/g)).toHaveLength(2);
    expect(operational.match(/qualityAttachmentScopeReadSql\(/g)).toHaveLength(1);
    expect(migration).toContain("qualityAttachmentScopeReadSql('legacy_hold')).bind(id).first");
    expect(policy).toContain("qualityAttachmentScopeReadSql('hold')).bind(facts.targetId,facts.companyId).first");
    expect(operational).toContain("qualityAttachmentScopeReadSql('spec')).bind(facts.targetId,facts.companyId).first");
  });

  test('native registry, CAS guards and all protected locks are untouched', () => {
    expect(read('adapters/native-quality-read.ts')).toContain('RD_ATTACH_HOLD_CAS');
    expect(read('adapters/native-quality-read.ts')).toContain('RD_ATTACH_SPEC_CAS');
    expect(read('adapters/native_quality_read_sql.rs')).toContain('EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=?)');
    expect(read('adapters/native_quality_read_sql.rs')).toContain('EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND company_id=? AND version=?)');
    expect(read('adapters/attachments-service-facts.ts')).toContain('RD_ATTACH_HOLD_CAS');
    expect(read('adapters/attachments-operational-facts.ts')).toContain('RD_ATTACH_SPEC_CAS');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/sales-package-sources.ts')).toContain('salesQualitySettlementSourceReadLocks');
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=24 baseline', () => {
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBeLessThan(24);
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(20);
    expect(kpi(/SELECT /g)).toBe(20);
  });
});
