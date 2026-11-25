import { describe, expect, test, vi, beforeEach } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
const { readFirst } = vi.hoisted(() => ({ readFirst: vi.fn() }));
vi.mock('../../../packages/server-quality/src/adapters/native-quality-read', () => ({
  RD_ATTACHMENT_DESCRIPTOR: 'quality_rd_attachment_descriptor',
  RD_LEGACY_ATTACHMENT_SCOPE: 'quality_rd_legacy_attachment_scope',
  RD_ATTACHMENT_SPEC_SCOPE: 'quality_rd_attachment_spec_scope',
  nativeQualityReadFirst: readFirst,
  nativeQualityGuardAssertion: vi.fn(),
}));
import { policyQualityFacts } from '../../../packages/server-quality/src/adapters/attachments-policy-facts';
import { legacyAttachmentScopequality } from '../../../packages/server-quality/src/adapters/attachment-migration-owner';
import { OperationalAttachmentQualityFacts } from '../../../packages/server-quality/src/adapters/attachments-operational-facts';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured before the native cutover. The TypeScript scope builder
// is deleted; these exact statements now live only in the owner read-plan
// registry, so the digests pin the native statements to the retired behavior.
const renderings = [
  {
    variant: 'legacy_hold',
    kind: 'quality_rd_legacy_attachment_scope',
    before: 'SELECT company_id,branch_id branch_id,NULL department_id,version FROM inventory_quality_holds WHERE id=?',
    digest: 'ee7c2b95215d5edfc4972070826b41aa4504e2c21909aeec7b84d171e15ca3a3',
    binds: 1,
  },
  {
    variant: 'spec',
    kind: 'quality_rd_attachment_spec_scope',
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

describe('quality read-tail r38 attachment scope-read native cutover', () => {
  test.each(renderings)('variant $variant is byte-identical in the native registry', (r) => {
    const registry = read('adapters/native_quality_read_sql.rs');
    expect(registry).toContain(`pub const ${r.variant === 'legacy_hold' ? 'RD_LEGACY_ATTACHMENT_SCOPE' : 'RD_ATTACHMENT_SPEC_SCOPE'}: &str = "${r.kind}"`);
    expect(registry).toContain(`"${r.before}"`);
    expect(sha(r.before)).toBe(r.digest);
    expect(r.before.match(/\?/g)).toHaveLength(r.binds);
    expect(r.before).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the TypeScript SQL builder is deleted and its adapters emit no SQL', () => {
    expect(() => read('adapters/quality-attachment-scope-read-sql.ts')).toThrow();
    const migration = read('adapters/attachment-migration-owner.ts');
    const policy = read('adapters/attachments-policy-facts.ts');
    const operational = read('adapters/attachments-operational-facts.ts');
    const scope = read('adapters/native-quality-scope.ts');
    for (const text of [migration, policy, operational, scope]) {
      expect(text).not.toContain('SELECT ');
      expect(text).not.toContain('qualityAttachmentScopeReadSql');
    }
    expect(migration).toContain('RD_LEGACY_ATTACHMENT_SCOPE');
    expect(operational).toContain('RD_ATTACHMENT_SPEC_SCOPE');
    expect(scope).toContain('readNativeQualityScope');
    expect(policy).toContain('nativeQualityReadFirst');
    expect(policy).toContain('RD_ATTACHMENT_DESCRIPTOR');
  });

  test('native registry, CAS guards and all protected locks are untouched', () => {
    expect(read('adapters/native-quality-read.ts')).toContain('RD_ATTACH_HOLD_CAS');
    expect(read('adapters/native-quality-read.ts')).toContain('RD_ATTACH_SPEC_CAS');
    expect(read('adapters/native-quality-read.ts')).toContain('RD_ATTACHMENT_DESCRIPTOR');
    expect(read('adapters/native_quality_read_sql.rs')).toContain('quality_rd_attachment_descriptor');
    expect(read('adapters/attachment_descriptor.rs')).toContain('DESCRIPTOR_CAPTURE_SQL');
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
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(16);
    expect(kpi(/SELECT /g)).toBe(16);
  });
});

describe('quality attachment descriptor native cutover', () => {
  beforeEach(() => { readFirst.mockReset(); });
  const row = { company_id: 'company', branch_id: 'branch', department_id: null, version: 7 };
  const db = {} as never;

  test('the native plan is byte-identical to the fenced owner port capture', () => {
    const registry = read('adapters/native_quality_read_sql.rs');
    const owner = read('adapters/attachment_descriptor.rs');
    const capture = owner.match(/pub const DESCRIPTOR_CAPTURE_SQL:&str="([^"]+)"/)?.[1];
    const plan = registry.match(/const SQL_ATTACHMENT_DESCRIPTOR: &str = "([^"]+)"/)?.[1];
    expect(capture).toBeDefined();
    expect(plan).toBe(capture!.replace(/\$1/g, '?').replace(/\$2/g, '?'));
    expect(registry).toContain('pub const RD_ATTACHMENT_DESCRIPTOR: &str = "quality_rd_attachment_descriptor"');
    expect(registry).toContain('RD_ATTACHMENT_DESCRIPTOR => Some(&READ_ATTACHMENT_DESCRIPTOR)');
    expect(registry).toContain('bind_count: 2,');
    expect(plan!.match(/\?/g)).toHaveLength(2);
    expect(plan).toContain('octet_length(company_id)+coalesce(octet_length(branch_id),0)<=2097152');
    expect(read('adapters/native-quality-read.ts')).toContain('export const RD_ATTACHMENT_DESCRIPTOR = "quality_rd_attachment_descriptor"');
  });

  test('admitted descriptor targets run on the native plan with exact binds', async () => {
    readFirst.mockResolvedValue({ ...row });
    for (const targetType of ['quality_hold', 'inventory_quality_hold']) {
      await expect(policyQualityFacts(db).readDescriptor({ targetId: 'hold-1', companyId: 'company', targetType })).resolves.toEqual(row);
      expect(readFirst).toHaveBeenLastCalledWith('quality_rd_attachment_descriptor', ['hold-1', 'company'], db);
    }
    await expect(policyQualityFacts(db).readQualityHold({ targetId: 'hold-1', companyId: 'company' })).resolves.toEqual(row);
    expect(readFirst).toHaveBeenLastCalledWith('quality_rd_attachment_descriptor', ['hold-1', 'company'], db);
  });

  test('unadmitted targets and missing rows return null without emitting SQL', async () => {
    await expect(policyQualityFacts(db).readDescriptor({ targetId: 'hold-1', companyId: 'company', targetType: 'inventory_quality_spec' })).resolves.toBeNull();
    expect(readFirst).not.toHaveBeenCalled();
    readFirst.mockResolvedValue(null);
    await expect(policyQualityFacts(db).readDescriptor({ targetId: 'missing', companyId: 'company', targetType: 'quality_hold' })).resolves.toBeNull();
  });

  test('malformed native descriptor rows fail closed', async () => {
    for (const bad of [
      { ...row, company_id: 1 },
      { ...row, branch_id: 7 },
      { ...row, department_id: 'unexpected' },
      { ...row, version: '7' },
      { ...row, version: 1.5 },
    ]) {
      readFirst.mockResolvedValue(bad);
      await expect(policyQualityFacts(db).readDescriptor({ targetId: 'hold-1', companyId: 'company', targetType: 'quality_hold' }))
        .rejects.toMatchObject({ code: 'QUALITY_NATIVE_DESCRIPTOR_RESPONSE_INVALID' });
    }
  });
});

describe('quality attachment owner-scope native cutover', () => {
  beforeEach(() => { readFirst.mockReset(); });
  const db = {} as never;
  const row = { company_id: 'company', branch_id: 'branch', department_id: null, version: 3 };

  test('legacy hold scope runs on the native plan with one bind', async () => {
    readFirst.mockResolvedValue({ ...row });
    await expect(legacyAttachmentScopequality(db, 'quality_hold', 'hold-1')).resolves.toEqual(row);
    expect(readFirst).toHaveBeenCalledWith('quality_rd_legacy_attachment_scope', ['hold-1'], db);
    await expect(legacyAttachmentScopequality(db, 'inventory_quality_spec', 'hold-1')).resolves.toBeNull();
    expect(readFirst).toHaveBeenCalledTimes(1);
  });

  test('spec scope runs on the native plan with company bind order intact', async () => {
    readFirst.mockResolvedValue({ ...row, branch_id: null });
    await expect(OperationalAttachmentQualityFacts(db).readInventoryQualitySpecs1({ targetId: 'spec-1', companyId: 'company' }))
      .resolves.toEqual({ ...row, branch_id: null });
    expect(readFirst).toHaveBeenCalledWith('quality_rd_attachment_spec_scope', ['spec-1', 'company'], db);
  });

  test('missing and malformed native scope rows stay closed', async () => {
    readFirst.mockResolvedValue(null);
    await expect(legacyAttachmentScopequality(db, 'quality_hold', 'missing')).resolves.toBeNull();
    for (const bad of [
      { ...row, company_id: 1 },
      { ...row, branch_id: 7 },
      { ...row, department_id: 9 },
      { ...row, version: '3' },
    ]) {
      readFirst.mockResolvedValue(bad);
      await expect(OperationalAttachmentQualityFacts(db).readInventoryQualitySpecs1({ targetId: 'spec-1', companyId: 'company' }))
        .rejects.toMatchObject({ code: 'QUALITY_NATIVE_SCOPE_RESPONSE_INVALID' });
    }
  });
});
