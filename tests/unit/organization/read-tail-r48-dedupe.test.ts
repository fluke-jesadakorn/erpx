import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { authoritySeatHolderGuard } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r48';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r48.ts');
const authority = read('department-authority.ts');

// Byte oracle. Only the two platform projection values and the owner-supplied
// scope/kind tail vary; the wrapper is rendered verbatim by the shared helper.
const PEOPLE = '<people>';
const ENGAGEMENTS = '<engagements>';
const ORACLE = (tail: string) =>
  `EXISTS(SELECT 1 FROM authority_seats s JOIN (${PEOPLE}) p ON p.id=s.person_id AND p.company_id=s.company_id JOIN (${ENGAGEMENTS}) e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' WHERE s.company_id=? AND s.${tail})`;

// Exact delegated call expressions and preserved tail expressions at each
// replaced call site (from the pre-r48 source).
const SITES: [string, string, string][] = [
  ['appointment-power',
    'authoritySeatHolderGuard(people_payrollPlatformSources.hr_people,people_payrollPlatformSources.hr_engagements,`scope_id=? AND s.kind=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND p.status=\'active\' AND p.merged_into_id IS NULL`)',
    "scope_id=? AND s.kind=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND p.status='active' AND p.merged_into_id IS NULL"],
  ['flow-publisher',
    'authoritySeatHolderGuard(people_payrollPlatformSources.hr_people,people_payrollPlatformSources.hr_engagements,`kind=? AND s.scope_id=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint`)',
    "kind=? AND s.scope_id=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint"],
];

describe('Organization read-tail r48 parameterized head', () => {
  test('r48 helper renders byte-identical SQL to every replaced literal', () => {
    expect(authoritySeatHolderGuard(PEOPLE, ENGAGEMENTS, '<T>')).toBe(ORACLE('<T>'));
    for (const [, , tail] of SITES) {
      expect(authoritySeatHolderGuard(PEOPLE, ENGAGEMENTS, tail)).toBe(ORACLE(tail));
    }
  });

  test('r48 defines one new helper and every r17-r47 helper is untouched', () => {
    expect(helper.match(/export const authoritySeatHolderGuard =/g) ?? []).toHaveLength(1);
    for (const [file, names] of [
      ['organization-column-alias-families.ts', ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']],
      ['organization-column-alias-families-r27.ts', ['activeNodeByKind', 'twoColumnLookup', 'nodeKindByIdCompany', 'versionedNodeActiveGuard']],
      ['organization-column-alias-families-r28.ts', ['companySnapshotAggregate', 'correlatedNodeJson']],
      ['organization-column-alias-families-r29.ts', ['maxVersionSubquery', 'impactCount']],
      ['organization-column-alias-families-r30.ts', ['activeLevelAbsent']],
      ['organization-column-alias-families-r31.ts', ['companyNodeByIdKind']],
      ['organization-column-alias-families-r34.ts', ['activeNodeKindExists']],
      ['organization-column-alias-families-r35.ts', ['ownerProjection']],
      ['organization-column-alias-families-r36.ts', ['activeNodeByIdCompanyKind']],
      ['organization-column-alias-families-r37.ts', ['leavePolicyVersionLookup']],
      ['organization-read-tail-sources.ts', ['departmentSnapshotMissing', 'ownerVersionGuardSource', 'NODE_BRANCH_ACTIVE', 'NODE_BRANCH_ACTIVE_EXISTS']],
      ['organization-read-tail-sources-r39.ts', ['companyNodeScopeSource', 'activeStaffLevelSource', 'changeVersionGuardSource', 'scopeNodeBranchExists']],
      ['organization-read-tail-sources-r40.ts', ['activePositionList', 'recommendedPayDefault', 'nodeChangeByIdCompany', 'activeStaffLevels', 'memberProfileByUser']],
      ['organization-read-tail-sources-r41.ts', ['relocationVersionSnapshot']],
      ['organization-read-tail-sources-r42.ts', ['activeCompanyNodes', 'branchOptionalId', 'latestDepartmentFlow', 'organizationChangeRows']],
      ['organization-read-tail-sources-r43.ts', ['activeNodeKindList']],
      ['organization-read-tail-sources-r44.ts', ['nodeSlugByKind']],
      ['organization-read-tail-sources-r45.ts', ['nodeChildExists']],
      ['organization-read-tail-sources-r46.ts', ['snapshotJsonByAction']],
      ['organization-read-tail-sources-r47.ts', ['workerCompanyRolloutGuard']],
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline head literals are gone, call sites delegate with unchanged binds, locks and native stay untouched', () => {
    // The inline authority-seat head is gone; only the helper renders it.
    expect(authority).not.toContain('EXISTS(SELECT 1 FROM authority_seats s JOIN (');
    expect((helper.match(/SELECT 1 FROM authority_seats s JOIN/g) ?? []).length).toBe(1);
    for (const [, call, tail] of SITES) {
      expect(authority.split(call).length - 1).toBe(1);
      expect(authority).toContain(tail);
    }
    // Bind arrays and the separate version guard stay byte-identical.
    expect(authority).toContain('[actor.companyId,actor.companyId,power,actor.userId]');
    expect(authority).toContain('[actor.companyId,authority,authority==="head"?input.departmentId:actor.companyId,actor.userId]');
    // r57 renders this version guard through the authority-seat EXISTS renderer.
    expect(authority).toContain("(?=0 AND NOT ${authoritySeatExistsSql('WHERE company_id=? AND kind=? AND scope_id=?')}) OR ${authoritySeatExistsSql('WHERE company_id=? AND kind=? AND scope_id=? AND version=?')}");
    // Native owner writes are untouched.
    expect(authority.split('nativeOrgStatement(').length - 1).toBe(4);

    // LOCK TABLE, FOR SHARE/FOR UPDATE and spaced JOIN constants stay byte-identical.
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r48 BEFORE=238 baseline (expected 196)', () => {
    // Wave 2 moved three node-reference reads to the native owner (six lane-23 reads total); the absolute pin reflects the current owner source.
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r48 collapses two authority-seat holder guards into one shared source (-1).
    expect(space).toBe(188);
    expect(space).toBeLessThan(238);
  });
});
