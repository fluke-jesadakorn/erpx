import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import {
  releaseAcceptanceRowSql,
  latestReleaseRowSql,
  releaseRevisionReadSql,
  releaseRevisionShareLockSql,
  releaseRevisionTable,
} from '../../../packages/server-platform_administration/src/adapters/release-predicate';
import { productionScope, legalProductionScope } from '@erpx/catalogue/production-scope';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

// Byte oracles captured from the BEFORE=28 source (d1ba45803e27…), asserted
// against the AFTER fold. They are rendered output only: neither row-head
// literal remains inline in the owner source.
const pre = {
  evidence: 'SELECT version,revoked_at FROM company_release_acceptances WHERE company_id=? AND id=?',
  finance: 'SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM company_release_acceptances WHERE id=? AND company_id=?',
  latestRowId: 'SELECT * FROM company_release_acceptances WHERE company_id=? AND id=? ORDER BY created_at DESC,id DESC LIMIT 1',
  latestRowScope: 'SELECT * FROM company_release_acceptances WHERE company_id=? AND scope_version=? ORDER BY created_at DESC,id DESC LIMIT 1',
  latestId: 'SELECT id FROM company_release_acceptances WHERE company_id=? AND scope_version=? ORDER BY created_at DESC,id DESC LIMIT 1',
  revisionProd: `SELECT revision FROM ${releaseRevisionTable(productionScope.version)} WHERE company_id=?`,
  revisionLegal: `SELECT revision FROM ${releaseRevisionTable(legalProductionScope.version)} WHERE company_id=?`,
  revisionProdShare: `SELECT revision FROM ${releaseRevisionTable(productionScope.version)} WHERE company_id=? FOR SHARE`,
  revisionLegalShare: `SELECT revision FROM ${releaseRevisionTable(legalProductionScope.version)} WHERE company_id=? FOR SHARE`,
  customSuffix: `SELECT revision FROM ${releaseRevisionTable(productionScope.version)} WHERE company_id=? /*probe*/`,
};

const preSha: Record<keyof typeof pre, string> = {
  evidence: '7bbf591b6ca0ae0ad49c1d5b46b2085e6cd29eecd8a11ca30c90919e1f233fa7',
  finance: '5682ddb79369b627509cec8b4733569289866b03caad0aeda5e884e7963ea8c6',
  latestRowId: '0a84d360fa79ecf5e4be6b065f6c07ad2492eec37d36a6382d13f27615b80b56',
  latestRowScope: 'fe0c8ad95e092a091f6216fef1daef7605eef8e53d325f76c70dfad15d45490d',
  latestId: 'f03e55f963f152000035aa5a27e6194908dea25cc704fff7a0cfdd93fd66dda4',
  revisionProd: '9f5d714fa5d12f33e63c1944be307c245d7f63f365c6592607d638006308d535',
  revisionLegal: 'd30fd7e6c5bfeaa372dcaaa88e7f0ad7b197746b138880b349767d629ca60629',
  revisionProdShare: '65be9d50336bc8ccb50630b1d868cde42d2b8ea18c06d60c4517d6afdc70e24f',
  revisionLegalShare: '62b5ed43cec04e1dc5f3455584833e6a654dc3a14a80792e78d3456a6c9b3e4c',
  customSuffix: '4cd9fea81df2ee86841677673b4f27a104be384f2566d72098d3eaec53687a28',
};

describe('Platform Administration read-tail r18 row-head fold', () => {
  test('every rendered statement keeps its exact bytes and bind count', () => {
    const after = {
      evidence: releaseAcceptanceRowSql('version,revoked_at', 'company_id=? AND id=?'),
      finance: releaseAcceptanceRowSql('company_id,NULL::text branch_id,NULL::text department_id,version', 'id=? AND company_id=?'),
      latestRowId: latestReleaseRowSql('*', 'AND id=?'),
      latestRowScope: latestReleaseRowSql('*', 'AND scope_version=?'),
      latestId: latestReleaseRowSql('id', 'AND scope_version=?'),
      revisionProd: releaseRevisionReadSql(productionScope.version),
      revisionLegal: releaseRevisionReadSql(legalProductionScope.version),
      revisionProdShare: releaseRevisionShareLockSql(productionScope.version),
      revisionLegalShare: releaseRevisionShareLockSql(legalProductionScope.version),
      customSuffix: releaseRevisionReadSql(productionScope.version, ' /*probe*/'),
    };
    for (const key of Object.keys(pre) as Array<keyof typeof pre>) {
      expect(after[key]).toBe(pre[key]);
      expect(sha(after[key])).toBe(preSha[key]);
      expect(after[key].split('SELECT ').length - 1).toBe(1);
    }
    expect(after.evidence.match(/\?/g)).toHaveLength(2);
    expect(after.finance.match(/\?/g)).toHaveLength(2);
    expect(after.revisionProd.match(/\?/g)).toHaveLength(1);
    expect(after.revisionLegal.match(/\?/g)).toHaveLength(1);
    expect(releaseRevisionTable(productionScope.version)).toBe('company_release_revisions');
    expect(releaseRevisionTable(legalProductionScope.version)).toBe('company_legal_release_revisions');
  });

  test('the two former heads are one parameterized row renderer', () => {
    const predicate = source('adapters/release-predicate.ts');
    expect(predicate.match(/export function releaseAcceptanceRowSql/g)).toHaveLength(1);
    expect(predicate.match(/export function releaseRevisionReadSql/g)).toHaveLength(1);
    expect(predicate.split('`SELECT ${columns} FROM ${table} WHERE ${predicate}${suffix}`').length - 1).toBe(1);
    expect(predicate.split('`SELECT ${columns} FROM company_release_acceptances WHERE ${predicate}${suffix}`').length - 1).toBe(0);
    expect(predicate.split('`SELECT revision FROM ${releaseRevisionTable(scope)} WHERE company_id=?${suffix}`').length - 1).toBe(0);
    expect(predicate).toContain("return releaseAcceptanceRowSql('revision', 'company_id=?', suffix, releaseRevisionTable(scope));");
    expect(predicate).toContain("export function releaseAcceptanceRowSql(columns: string, predicate: string, suffix = '', table = 'company_release_acceptances')");
    expect(predicate).toContain("releaseRevisionReadSql(scope, ' FOR SHARE')");
    expect(predicate).not.toMatch(/SELECT revision FROM \$\{releaseRevisionTable\(scope\)\}/);
    expect(predicate.split('SELECT ').length - 1).toBe(9);
  });

  test('callers and their bind arrays are unchanged', () => {
    const acceptance = source('adapters/release-acceptance.ts');
    const finance = source('adapters/attachments-finance-facts.ts');
    expect(acceptance).toContain("releaseAcceptanceRowSql('version,revoked_at','company_id=? AND id=?')");
    expect(acceptance).toContain("latestReleaseRowSql('*', releaseId?'AND id=?':'AND scope_version=?')");
    expect(acceptance).toContain('releaseRevisionReadSql(scope)');
    expect(finance).toContain("releaseAcceptanceRowSql('company_id,NULL::text branch_id,NULL::text department_id,version','id=? AND company_id=?')");
    expect(acceptance.match(/\[actor\.companyId,\s*id,\s*input\.expectedVersion\]/g)).toHaveLength(3);
    expect(acceptance).toContain('[actor.companyId, id, actor.companyId, scope]');
    expect(finance).toContain('.bind(facts.targetId,facts.companyId)');
  });

  test('locks, authority, native reads, pg_catalog fence and cross-owner sources are untouched', () => {
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/collaboration-reply-rollout.ts')).toContain('SELECT company_id FROM engineering_verification_contexts WHERE company_id=? FOR SHARE');
    expect(source('adapters/company-release-attachment-read-sources.ts')).toContain('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE');
    expect(source('adapters/engineering-verification.ts')).toContain('/*erpx-native-engineering-fence*/');
    expect(source('adapters/engineering-verification.ts')).toContain('pg_catalog.pg_database');
    expect(source('adapters/native-owner-write.ts')).toContain('export const NATIVE_OWNER_WRITE_NOOP_SQL');
    expect(source('adapters/release-predicate.ts')).toContain('iam_assignment_active(reviewer.id');
    expect(source('adapters/release-predicate.ts')).toContain('organizationPlatformSources.company_business_acceptance');
    expect(source('adapters/release-predicate.ts')).toContain('identity_accessPlatformSources.assignments');
  });
});

describe('Platform Administration r18 retirement check', () => {
  test('retirement:ts-statements is pinned at the post-dedupe 27 baseline (BEFORE=28)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(27);
    expect(total).toBeLessThan(28);
  });
});
