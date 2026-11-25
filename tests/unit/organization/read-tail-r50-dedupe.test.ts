import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  nodeExistsGuard,
  activePositionLookup,
} from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r50';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r50.ts');
const org = read('organization.ts');
const planning = read('planning-resource-sources.ts');
const placement = read('people-hire-placement-facts.ts');
const directory = read('people-directory-facts.ts');
const staff = read('staff-defaults.ts');
const evaluation = read('people-evaluation-facts.ts');
const gpo = read('gpo-sources.ts');

// Byte oracles recorded verbatim from the pre-r50 (BEFORE=234) source. Only
// caller-supplied tails and projections vary; each head is rendered verbatim
// by its helper. The active-position family's literal head is
// `FROM organization_positions WHERE id=? AND company_id=? AND ` — the
// people-evaluation site puts `level_id=?` before `active=1`, so `active=1`
// belongs to its tail, not to the byte-shared head.
const NODE_HEAD = 'EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND ';
const nodeOracle = (tail: string) => `${NODE_HEAD}${tail})`;
const POS_HEAD = ' FROM organization_positions WHERE id=? AND company_id=? AND ';
const posOracle = (projection: string, tail: string) => `SELECT ${projection}${POS_HEAD}${tail}`;

const NODE_SITES: [file: string, tail: string, source: string][] = [
  ['organization.ts', 'hierarchy_revision=? AND kind=? AND branch_id IS NOT DISTINCT FROM ?', org],
  ['planning-resource-sources.ts', "kind IN ('branch','headquarters') AND status='active'", planning],
  ['people-hire-placement-facts.ts', "status='active'", placement],
  ['people-hire-placement-facts.ts', "branch_id=? AND status='active'", placement],
  ['people-directory-facts.ts', "branch_id=? AND kind='department' AND status='active'", directory],
];
const POS_SITES: [file: string, projection: string, tail: string, source: string][] = [
  ['staff-defaults.ts', 'id', 'active=1 AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)', staff],
  ['gpo-sources.ts', 'id,title', 'active=1', gpo],
];

describe('Organization read-tail r50 dedupe', () => {
  test('r50 helpers render byte-identical SQL to every replaced literal', () => {
    expect(nodeExistsGuard('<T>')).toBe(nodeOracle('<T>'));
    expect(activePositionLookup('<P>', '<T>')).toBe(posOracle('<P>', '<T>'));
    for (const [, tail] of NODE_SITES) expect(nodeExistsGuard(tail)).toBe(nodeOracle(tail));
    for (const [, projection, tail] of POS_SITES) expect(activePositionLookup(projection, tail)).toBe(posOracle(projection, tail));

    // r53 retired the two inline remainders that r50 had left intentional: the
    // optional hire-placement EXISTS and the department guard. Both tails are
    // rendered verbatim by nodeExistsGuard; only their call sites vary.
    const PLACEMENT_ORACLE = "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND status='active') AND (?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND status='active'))";
    expect(`${nodeExistsGuard("status='active'")} AND (?::text IS NULL OR ${nodeExistsGuard("branch_id=? AND status='active'")})`).toBe(PLACEMENT_ORACLE);

    const DIRECTORY_ORACLE = "?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND kind='department' AND status='active')";
    expect(`?::text IS NULL OR ${nodeExistsGuard("branch_id=? AND kind='department' AND status='active'")}`).toBe(DIRECTORY_ORACLE);
  });

  test('r50 defines two new helpers and every r17-r49 helper is untouched', () => {
    for (const name of ['nodeExistsGuard', 'activePositionLookup']) {
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
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline heads are gone; call sites delegate with unchanged binds, locks and native', () => {
    for (const [, tail, source] of NODE_SITES) expect(source).not.toContain(`${NODE_HEAD}${tail})`);
    for (const [, projection, tail, source] of POS_SITES) expect(source).not.toContain(posOracle(projection, tail));

    expect(helper.split(NODE_HEAD).length - 1).toBe(1);
    expect(helper.split(POS_HEAD).length - 1).toBe(1);
    for (const [file, , source] of NODE_SITES) {
      expect(source.split('nodeExistsGuard(').length - 1).toBe(NODE_SITES.filter(([f]) => f === file).length);
      expect(read(file)).toBe(source);
    }
    for (const [file, , , source] of POS_SITES) {
      expect(source.split('activePositionLookup(').length - 1).toBe(1);
      expect(read(file)).toBe(source);
    }
    // Lane 24: the evaluation site is now a strict native-client read, with no
    // helper call and no local statement in the adapter.
    expect(evaluation).not.toContain('activePositionLookup(');
    expect(evaluation).not.toContain('.prepare(');
    expect(evaluation).toContain('nativeLane24PositionByLevel');

    // Bind arrays stay byte-identical at every replaced call site.
    expect(org).toContain("[change.target_node_id,actor.companyId,change.expected_node_version!,change.node_kind,change.action==='relocate'?change.source_branch_id:change.branch_id]");
    expect(planning).toContain('[branchId, companyId]');
    expect(placement).toContain('[branch_id, companyId, department_id, department_id2, companyId2, branch_id2]');
    expect(staff).toContain('.bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first()');
    expect(evaluation).toContain('nativeLane24PositionByLevel(target, companyId, positionId, levelId)');
    expect(gpo).toContain('.bind(positionId,companyId).first<{id:string;title:string}>()');

    // The r50 intentional remainders are gone; their exact bodies are now
    // rendered by the helper (asserted byte-for-byte above).
    expect(placement).not.toContain("EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND status='active')");
    expect(directory).not.toContain("EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND kind='department' AND status='active')");
    expect(org.split('nativeOrgStatement(').length - 1).toBe(15);
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r50 BEFORE=234 baseline (expected 196)', () => {
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
    // r50 collapsed three node-existence guards and three active-position
    // lookups into two shared helpers (-2, -2 = -4); r53 routes the two
    // remaining inline node-EXISTS bodies through nodeExistsGuard (-2).
    expect(space).toBe(179);
    expect(space).toBeLessThan(234);
  });
});
