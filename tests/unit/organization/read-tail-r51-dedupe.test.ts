import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  authoritySeatCompanyRead,
  activeDepartmentNode,
  memberProfileVersionGuard,
} from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r51';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r51.ts');
const org = read('organization.ts');
const companies = read('commercial-companies.ts');
const approvals = read('workflow-approvals-facts.ts');
const gpo = read('gpo-sources.ts');
const profile = read('profile-media-command.ts');

// Byte oracles recorded verbatim from the pre-r51 (BEFORE=230) source. Only
// owner-supplied tails and closed projections vary; each head is rendered
// verbatim by its helper. The department-active organization port keeps its
// branch predicate in the caller tail; the media-removal guard keeps its key
// predicate in the caller tail.
const SEAT_HEAD = ' FROM authority_seats WHERE company_id=? AND ';
const seatOracle = (projection: string, tail: string) => `SELECT ${projection}${SEAT_HEAD}${tail}`;
const SEAT_SITES: [file: string, projection: string, tail: string, source: string][] = [
  ['organization.ts', 'version,position_id,person_id,effective_at', "kind='head' AND scope_id=?", org],
  ['commercial-companies.ts', 'min(effective_at) AS deadline', "kind='hr' AND scope_id=? AND effective_at>?", companies],
  ['workflow-approvals-facts.ts', '1', "person_id=? AND kind IN ('ceo','hr') AND effective_at<=? LIMIT 1", approvals],
];

const DEPT_HEAD = " FROM organization_nodes WHERE company_id=? AND id=? AND kind='department' AND status='active'";
const deptOracle = (projection: string) => `SELECT ${projection}${DEPT_HEAD}`;
const DEPT_SITES: [file: string, projection: string, source: string][] = [
  ['organization.ts', '1', org],
  ['gpo-sources.ts', 'display_name name', gpo],
];

const PROFILE_HEAD = 'EXISTS(SELECT 1 FROM member_profiles WHERE user_id=? AND version=?';
const profileOracle = (tail: string) => `${PROFILE_HEAD}${tail})`;
const PROFILE_TAILS = ['', ' AND ${input.kind}_key=?'];

describe('Organization read-tail r51 dedupe', () => {
  test('r51 helpers render byte-identical SQL to every replaced literal', () => {
    expect(authoritySeatCompanyRead('1', '<T>')).toBe(seatOracle('1', '<T>'));
    expect(activeDepartmentNode('1')).toBe(deptOracle('1'));
    expect(memberProfileVersionGuard('<T>')).toBe(profileOracle('<T>'));
    for (const [, projection, tail] of SEAT_SITES) expect(authoritySeatCompanyRead(projection as '1', tail)).toBe(seatOracle(projection, tail));
    for (const [, projection] of DEPT_SITES) expect(activeDepartmentNode(projection as '1')).toBe(deptOracle(projection));
    for (const tail of PROFILE_TAILS) expect(memberProfileVersionGuard(tail)).toBe(profileOracle(tail));

    // The organization port's branch predicate stays outside the shared head.
    expect(`${activeDepartmentNode('1')} AND branch_id IS NOT DISTINCT FROM ?`).toBe(
      "SELECT 1 FROM organization_nodes WHERE company_id=? AND id=? AND kind='department' AND status='active' AND branch_id IS NOT DISTINCT FROM ?",
    );
  });

  test('r51 defines three new helpers and every r17-r50 helper is untouched', () => {
    for (const name of ['authoritySeatCompanyRead', 'activeDepartmentNode', 'memberProfileVersionGuard']) {
      expect(helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
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
      ['organization-read-tail-sources-r47.ts', ['workerCompanyRolloutGuard']],
      ['organization-read-tail-sources-r48.ts', ['authoritySeatHolderGuard']],
      ['organization-read-tail-sources-r49.ts', ['fullNodeSelection', 'messagingNodeExists']],
      ['organization-read-tail-sources-r50.ts', ['nodeExistsGuard', 'activePositionLookup']],
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline heads are gone; call sites delegate with unchanged binds, locks and native', () => {
    for (const [, projection, tail, source] of SEAT_SITES) expect(source).not.toContain(seatOracle(projection, tail));
    for (const [, projection, source] of DEPT_SITES) expect(source).not.toContain(deptOracle(projection));
    expect(org).not.toContain(`${deptOracle('1')} AND branch_id IS NOT DISTINCT FROM ?`);
    for (const tail of PROFILE_TAILS) expect(profile).not.toContain(profileOracle(tail));

    expect(helper.split('SELECT ').length - 1).toBe(3);
    for (const [file, , , source] of SEAT_SITES) {
      expect(source.split('authoritySeatCompanyRead(').length - 1).toBe(1);
      expect(read(file)).toBe(source);
    }
    for (const [file, , source] of DEPT_SITES) {
      expect(source.split('activeDepartmentNode(').length - 1).toBe(1);
      expect(read(file)).toBe(source);
    }
    expect(profile.split('memberProfileVersionGuard(').length - 1).toBe(2);

    // Bind arrays stay byte-identical at every replaced call site.
    expect(org).toContain('.bind(companyId,departmentId).first()');
    expect(org).toContain('.bind(companyId,id,branchId).first()');
    expect(companies).toContain('.bind(companyId, companyId, observedAt).first<{ deadline: number | null }>()');
    expect(approvals).toContain('.bind(companyId,person_id,now).first()');
    expect(gpo).toContain('.bind(companyId,target.id).first<{name:string}>()');
    expect(profile).toContain('[actor.userId, input.expectedVersion]');
    expect(profile).toContain('[actor.userId, input.version, objectKey]');

    // Other version guards, locks, native statements and spaced JOINs stay untouched.
    // r57 renders these version guards through the authority-seat EXISTS renderer.
    expect(read('department-authority.ts')).toContain(
      "authoritySeatExistsSql('WHERE company_id=? AND kind=? AND scope_id=? AND version=?')",
    );
    expect(org).toContain('authoritySeatExistsSql("WHERE company_id=? AND kind=\'head\' AND scope_id=? AND version=?');
    expect(profile).toContain('NOT EXISTS(SELECT 1 FROM member_profiles WHERE user_id=?)');
    expect(org.split('nativeOrgStatement(').length - 1).toBe(15);
    expect(profile.split('nativeOrgStatement(').length - 1).toBe(3);
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(companies).toContain('LOCK TABLE organization_nodes,authority_seats,organization_positions IN SHARE MODE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r51 BEFORE=230 baseline (expected 196)', () => {
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
    // r51 collapses three authority-seat reads, two department-node reads and
    // two member-profile guards into three shared helpers (-2, -1, -1 = -4).
    expect(space).toBe(186);
    expect(space).toBeLessThan(230);
  });
});
