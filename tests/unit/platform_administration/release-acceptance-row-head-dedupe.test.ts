import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { latestReleaseRowSql, releaseAcceptanceRowSql } from '../../../modules/platform-administration/server/src/adapters/release-predicate';

const srcRoot = resolve(__dirname, '../../../modules/platform-administration/server/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');

// Pre-change renderings copied verbatim from the BEFORE=33 source. They are
// byte-identity oracles only: none of these statements remains inline.
const preRow = (idPredicate: string) => `SELECT * FROM company_release_acceptances WHERE company_id=? ${idPredicate} ORDER BY created_at DESC,id DESC LIMIT 1`;
const preLatest = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND scope_version=? ORDER BY created_at DESC,id DESC LIMIT 1';
const preEvidence = 'SELECT version,revoked_at FROM company_release_acceptances WHERE company_id=? AND id=?';
const preFinance = 'SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM company_release_acceptances WHERE id=? AND company_id=?';

describe('Platform Administration release-acceptance row SELECT-head family dedupe (r9)', () => {
  test('the one parameterized head renders byte-identically for every family caller', () => {
    expect(releaseAcceptanceRowSql('version,revoked_at', 'company_id=? AND id=?')).toBe(preEvidence);
    expect(releaseAcceptanceRowSql('company_id,NULL::text branch_id,NULL::text department_id,version', 'id=? AND company_id=?')).toBe(preFinance);
    expect(latestReleaseRowSql('*', 'AND id=?')).toBe(preRow('AND id=?'));
    expect(latestReleaseRowSql('*', 'AND scope_version=?')).toBe(preRow('AND scope_version=?'));
    expect(latestReleaseRowSql('id', 'AND scope_version=?')).toBe(preLatest);
  });

  test('the shared head and its suffix composition survive exactly once', () => {
    const predicate = source('adapters/release-predicate.ts');
    const acceptance = source('adapters/release-acceptance.ts');
    const finance = source('adapters/attachments-finance-facts.ts');
    const head = 'FROM ${table} WHERE ${predicate}${suffix}';
    expect(predicate.split(head).length - 1).toBe(1);
    expect(predicate.match(/export function releaseAcceptanceRowSql/g)).toHaveLength(1);
    expect(predicate.match(/export function latestReleaseRowSql/g)).toHaveLength(1);
    expect(acceptance).toContain("releaseAcceptanceRowSql('version,revoked_at','company_id=? AND id=?')");
    expect(finance).toContain("releaseAcceptanceRowSql('company_id,NULL::text branch_id,NULL::text department_id,version','id=? AND company_id=?')");
    expect(`${predicate}\n${acceptance}\n${finance}`).not.toContain(preEvidence);
    expect(`${predicate}\n${acceptance}\n${finance}`).not.toContain(preFinance);
  });

  test('locks and native reads are untouched', () => {
    const predicate = source('adapters/release-predicate.ts');
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/collaboration-reply-rollout.ts')).toContain("releaseAcceptanceRowSql('company_id','company_id=?',' FOR SHARE','engineering_verification_contexts')");
    expect(source('adapters/company-release-attachment-read-sources.ts')).toContain('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE');
    expect(source('adapters/engineering-verification.ts')).not.toContain('SELECT source_fingerprint,software_version,release_manifest_sha256 FROM engineering_verification_contexts WHERE company_id=? AND run_id=?');
    expect(source('adapters/engineering-verification.ts')).toContain('nativeEngineeringPriorRead(companyId, run)');
    expect(source('adapters/native_lane_33/postgres.rs')).toContain('jsonb_build_object(');
    for (const helper of ['thbCurrencyProbeSql', 'releaseAcceptanceExistsSql', 'latestReleaseIdSql', 'activeBranchCountSql', 'businessAcceptanceSetupSql', 'releaseRevisionReadSql', 'releaseRevisionShareLockSql']) {
      expect(predicate + source('adapters/company-release-attachment-read-sources.ts')).toContain(`export function ${helper}`);
    }
  });

  test('retirement:ts-statements stays strictly below the r9 BEFORE=33 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(33);
  });
});
