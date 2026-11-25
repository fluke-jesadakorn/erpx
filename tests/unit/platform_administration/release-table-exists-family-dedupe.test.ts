import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { productionScope } from '@erpx/catalogue/production-scope';
import { releaseRevisionTable, releaseTableExistsSql } from '../../../packages/server-platform_administration/src/adapters/release-predicate';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');

// Pre-dedupe renderings copied verbatim from the BEFORE=32 source. They are
// byte-identity oracles only: neither statement remains inline after r10.
const preFeatureRelease = (rev: string) => `EXISTS(SELECT 1 FROM company_release_acceptances r JOIN ${rev} c ON c.company_id=r.company_id
          WHERE r.id=? AND r.company_id=? AND r.version=? AND r.revoked_at IS NULL AND r.configuration_revision=c.revision
          AND r.scope_version=? AND r.software_version=?)`;
const preConfiguration = (rev: string) => `EXISTS(SELECT 1 FROM ${rev} WHERE company_id=? AND revision=?)`;

describe('Platform Administration release-table EXISTS SELECT-head family dedupe (r10)', () => {
  test('the one parameterized SELECT head renders byte-identically for both family callers', () => {
    const rev = releaseRevisionTable(productionScope.version);
    expect(releaseTableExistsSql(rev, 'company_id=? AND revision=?')).toBe(preConfiguration(rev));
    expect(releaseTableExistsSql(
      `company_release_acceptances r JOIN ${rev} c ON c.company_id=r.company_id`,
      `r.id=? AND r.company_id=? AND r.version=? AND r.revoked_at IS NULL AND r.configuration_revision=c.revision
          AND r.scope_version=? AND r.software_version=?`,
      `
          `,
    )).toBe(preFeatureRelease(rev));
    expect(releaseTableExistsSql('t', 'x=1')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the head survives once and both guards are composed through it', () => {
    const predicate = source('adapters/release-predicate.ts');
    const acceptance = source('adapters/release-acceptance.ts');
    const pms = source('adapters/pms-rollout-guards.ts');
    const all = `${predicate}\n${acceptance}\n${pms}`;
    expect(predicate.match(/export function releaseTableExistsSql/g)).toHaveLength(1);
    expect(predicate.split('`SELECT 1 FROM ${from}${gap}WHERE ${where}`').length - 1).toBe(1);
    expect(predicate).toContain('return `EXISTS(${selectOneFromWhereSql(from, where, gap)})`;');
    expect(acceptance).toContain("releaseTableExistsSql(releaseRevisionTable(scope), 'company_id=? AND revision=?')");
    expect(pms).toContain('releaseTableExistsSql(`company_release_acceptances r JOIN ${releaseRevisionTable(scope)} c ON c.company_id=r.company_id`');
    expect(all).not.toContain(preConfiguration('${releaseRevisionTable(scope)}'));
  });

  test('locks, native reads and prior helpers are untouched', () => {
    const predicate = source('adapters/release-predicate.ts');
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/collaboration-reply-rollout.ts')).toContain('SELECT company_id FROM engineering_verification_contexts WHERE company_id=? FOR SHARE');
    expect(source('adapters/engineering-verification.ts')).not.toContain('SELECT source_fingerprint,software_version,release_manifest_sha256 FROM engineering_verification_contexts WHERE company_id=? AND run_id=?');
    expect(source('adapters/engineering-verification.ts')).toContain('nativeEngineeringPriorRead(companyId, run)');
    for (const helper of ['releaseAcceptanceExistsSql', 'latestReleaseIdSql', 'activeBranchCountSql', 'businessAcceptanceSetupSql', 'releaseRevisionReadSql', 'releaseRevisionShareLockSql', 'releaseAcceptanceRowSql', 'latestReleaseRowSql', 'thbCurrencyProbeSql', 'releaseCurrencyPredicate']) {
      expect(predicate).toContain(`export function ${helper}`);
    }
    expect(source('adapters/company-release-attachment-read-sources.ts')).toContain('readNativeReleaseChildImage(actor,id)');
    expect(source('adapters/native-owner-write.ts')).toContain('export function selectOneSql');
  });

  test('retirement:ts-statements stays strictly below the r10 BEFORE=32 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(20);
  });
});
