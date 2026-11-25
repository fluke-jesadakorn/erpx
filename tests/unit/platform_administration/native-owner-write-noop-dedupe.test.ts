import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import { NATIVE_OWNER_WRITE_NOOP_SQL, selectOneSql } from '../../../packages/server-platform_administration/src/adapters/native-owner-write';
import { businessAcceptanceSetupSql, releaseRevisionReadSql, releaseRevisionTable } from '../../../packages/server-platform_administration/src/adapters/release-predicate';
import { productionScope } from '@erpx/catalogue/production-scope';
import { platformOwnerProjectionSources as organizationPlatformSources } from '../../../packages/server-organization/src/public/platform-owner-projection-sources';

const source = (relative: string) =>
  readFileSync(new URL(`../../../packages/server-platform_administration/src/${relative}`, import.meta.url), 'utf8');

describe('platform administration native owner-write no-op dedupe', () => {
  it('keeps exactly one canonical inert no-op SELECT', () => {
    expect(NATIVE_OWNER_WRITE_NOOP_SQL).toBe('SELECT 1');
    expect(NATIVE_OWNER_WRITE_NOOP_SQL.match(/SELECT /g)).toHaveLength(1);
  });

  it('shares one owner-local one-row probe base with byte-identical suffixes', () => {
    expect(selectOneSql()).toBe(NATIVE_OWNER_WRITE_NOOP_SQL);
    expect(selectOneSql(' WHERE probe')).toBe('SELECT 1 WHERE probe');
    const base = source('adapters/native-owner-write.ts');
    expect(base.split('`SELECT 1${suffix}`').length - 1).toBe(1);
    const engineering = source('adapters/engineering-verification.ts');
    expect(engineering).toContain('selectOneSql(` WHERE ${predicate}`)');
    expect(engineering).not.toContain('`SELECT 1 WHERE ${predicate}`');
  });

  it('reuses the canonical no-op in every native owner-write caller', () => {
    for (const file of ['adapters/release-acceptance.ts', 'adapters/attachments-finance-facts.ts']) {
      const text = source(file);
      expect(text).toContain('NATIVE_OWNER_WRITE_NOOP_SQL');
      expect(text).not.toContain('db.prepare("SELECT 1")');
    }
  });

  it('keeps one canonical release-version guard and one canonical row lock', () => {
    const text = source('adapters/release-acceptance.ts');
    const versionGuardWhere = "company_id=? AND id=? AND version=? AND revoked_at IS NULL";
    const rowLock = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE';
    expect(text).toContain(`export const RELEASE_VERSION_GUARD_SQL = releaseAcceptanceExistsSql('${versionGuardWhere}')`);
    expect(text).toContain(`export const RELEASE_ROW_LOCK_SQL = '${rowLock}'`);
    expect(text.match(/RELEASE_VERSION_GUARD_SQL/g)).toHaveLength(4); // declaration + review, revoke, evidence guards
    expect(text.match(/RELEASE_ROW_LOCK_SQL/g)).toHaveLength(3); // declaration + review, revoke locks
    expect(text.match(new RegExp(versionGuardWhere.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'))).toHaveLength(1);
    expect(text.match(new RegExp(rowLock.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'))).toHaveLength(1);
  });

  it('keeps one canonical revision read and reuses it in the share-lock, PMS and release acceptance', () => {
    const revisionRead = 'SELECT revision FROM ${releaseRevisionTable(scope)} WHERE company_id=?';
    const all = [
      'adapters/release-predicate.ts',
      'adapters/pms-rollout-guards.ts',
      'adapters/release-acceptance.ts',
    ].map(source).join('\n');
    expect(all.split(revisionRead).length - 1).toBe(0);
    expect(source('adapters/release-predicate.ts')).toContain("return releaseAcceptanceRowSql('revision', 'company_id=?', suffix, releaseRevisionTable(scope));");
    expect(source('adapters/release-predicate.ts')).toContain("releaseRevisionReadSql(scope, ' FOR SHARE')");
    expect(source('adapters/release-acceptance.ts')).toContain('releaseRevisionReadSql(scope)');
    for (const file of ['adapters/pms-rollout-guards.ts', 'adapters/release-acceptance.ts']) {
      const text = source(file);
      expect(text).toContain('releaseRevisionShareLockSql(scope)');
      expect(text).not.toContain('WHERE company_id=? FOR SHARE');
    }
    expect(source('public/release-predicate.ts')).toContain('releaseRevisionShareLockSql');
  });

  it('keeps one canonical latest-release-id subquery and reuses it in release guards and predicate', () => {
    const latestBody = 'SELECT latest.id FROM company_release_acceptances latest WHERE latest.company_id=';
    const predicate = source('adapters/release-predicate.ts');
    const acceptance = source('adapters/release-acceptance.ts');
    expect(`${predicate}\n${acceptance}`.split(latestBody).length - 1).toBe(1);
    expect(predicate).toContain("latestReleaseIdSql('release.company_id', 'release.scope_version')");
    expect(acceptance).toContain("latestReleaseIdSql('?', '?')");
    expect(acceptance).not.toContain('WHERE latest.company_id=?');
  });

  it('keeps one canonical active-branch count subquery and reuses it in guards, inputs and predicate', () => {
    const branchBody = "organization_nodes) organization_nodes WHERE company_id=? AND kind='branch' AND status='active'";
    const predicate = source('adapters/release-predicate.ts');
    const acceptance = source('adapters/release-acceptance.ts');
    // The branch head is now composed through the shared projection-source head.
    expect(predicate).toContain('projectionSourceHeadSql(count, organizationPlatformSources.organization_nodes,');
    expect(`${predicate}\n${acceptance}`.split("organization_nodes}) ${options.qualifier ?? 'organization_nodes'} WHERE").length - 1).toBe(0);
    expect(predicate).toContain("activeBranchCountSql('release.company_id', { qualifier: 'branch' })");
    expect(acceptance).toContain("activeBranchCountSql('?')");
    expect(acceptance).toContain("activeBranchCountSql('?', { countAlias: 'count' })");
    expect(acceptance).not.toContain(branchBody);
    expect(predicate).not.toContain("branch WHERE branch.company_id=release.company_id");
  });

  it('keeps one canonical business-acceptance setup projection read and reuses it in release guards and inputs', () => {
    const setupBody = "FROM (${organizationPlatformSources.company_business_acceptance}) company_business_acceptance WHERE company_id=";
    const predicate = source('adapters/release-predicate.ts');
    const acceptance = source('adapters/release-acceptance.ts');
    // The setup projection head is now composed through the shared projection-source head.
    expect(predicate).toContain("projectionSourceHeadSql(columns, organizationPlatformSources.company_business_acceptance, 'company_business_acceptance'");
    expect(`${predicate}\n${acceptance}`.split(setupBody).length - 1).toBe(0);
    expect(predicate).toContain("businessAcceptanceSetupSql");
    expect(acceptance).toContain("businessAcceptanceSetupSql('?', { acceptedVersion: '?' })");
    expect(acceptance).toContain("businessAcceptanceSetupSql('?', { columns: 'version,status' })");
    expect(acceptance).not.toContain("SELECT 1 FROM (${organizationPlatformSources.company_business_acceptance})");
    expect(acceptance).not.toContain("SELECT version,status FROM (${organizationPlatformSources.company_business_acceptance})");
  });

  it('generates byte-identical setup SQL for the guard and the inputs read', () => {
    const projection = organizationPlatformSources.company_business_acceptance;
    expect(businessAcceptanceSetupSql('?', { acceptedVersion: '?' })).toBe(
      `SELECT 1 FROM (${projection}) company_business_acceptance WHERE company_id=? AND status='accepted' AND version=?`,
    );
    expect(businessAcceptanceSetupSql('?', { columns: 'version,status' })).toBe(
      `SELECT version,status FROM (${projection}) company_business_acceptance WHERE company_id=?`,
    );
  });

  it('generates byte-identical revision read SQL with the caller suffix appended', () => {
    const scope = productionScope.version;
    const base = releaseRevisionReadSql(scope);
    expect(base).toBe(`SELECT revision FROM ${releaseRevisionTable(scope)} WHERE company_id=?`);
    expect(releaseRevisionReadSql(scope, ' FOR SHARE')).toBe(`${base} FOR SHARE`);
  });
});
