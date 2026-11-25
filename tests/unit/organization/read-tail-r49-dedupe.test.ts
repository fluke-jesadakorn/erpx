import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  fullNodeSelection,
  messagingNodeExists,
} from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r49';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r49.ts');
const pms = read('pms-command-facts.ts');
const pos = read('pos-facts.ts');
const org = read('organization.ts');
const messaging = read('collaboration-messaging-projections.ts');

// Byte oracles recorded verbatim from the pre-r49 (BEFORE=237) source. Only
// caller-supplied tails vary; each head is rendered verbatim by its helper.
const NODE_HEAD =
  'SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision FROM organization_nodes WHERE ';
const nodeOracle = (where: string) => `${NODE_HEAD}${where}`;
const EXISTS_HEAD = 'EXISTS(SELECT 1 FROM organization_nodes n WHERE ';
const existsOracle = (predicate: string) => `${EXISTS_HEAD}${predicate})`;

// Tails and exact delegated call expressions from the pre-r49 source. The
// template-literal sites are escaped so the `${...}` text stays verbatim.
const NODE_SITES: [call: string, where: string][] = [
  [
    `fullNodeSelection("company_id=? AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR kind!='department' OR id=?) ORDER BY status,display_name")`,
    `company_id=? AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR kind!='department' OR id=?) ORDER BY status,display_name`,
  ],
  [
    `fullNodeSelection("id=? AND company_id=? AND kind IN ('branch','department')")`,
    `id=? AND company_id=? AND kind IN ('branch','department')`,
  ],
];
const MANAGED_PREDICATE = `n.company_id=\${alias}.company_id AND n.kind='company' AND n.status='active' AND \${organizationMemberSql('n',user)}`;
const ROOM_PREDICATE = `n.id=\${alias}.organization_node_id AND n.company_id=\${alias}.company_id AND n.status='active' AND \${organizationMemberSql('n',user)}`;
const MESSAGE_SITES: [call: string, predicate: string][] = [
  [`messagingNodeExists(\`${MANAGED_PREDICATE}\`)`, MANAGED_PREDICATE],
  [`messagingNodeExists(\`${ROOM_PREDICATE}\`)`, ROOM_PREDICATE],
];

describe('Organization read-tail r49 dedupe', () => {
  test('r49 helpers render byte-identical SQL to every replaced literal', () => {
    expect(fullNodeSelection('<W>')).toBe(nodeOracle('<W>'));
    expect(messagingNodeExists('<P>')).toBe(existsOracle('<P>'));
    for (const [, where] of NODE_SITES) expect(fullNodeSelection(where)).toBe(nodeOracle(where));
    for (const [, predicate] of MESSAGE_SITES) expect(messagingNodeExists(predicate)).toBe(existsOracle(predicate));
  });

  test('r49 defines two remaining helpers and every r17-r48 helper is untouched', () => {
    for (const name of ['fullNodeSelection', 'messagingNodeExists']) {
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
      ['organization-column-alias-families-r38.ts', ['activePositionById']],
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
      ['organization-read-tail-sources-r48.ts', ['authoritySeatHolderGuard']],
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline heads are gone; call sites delegate with unchanged binds, locks and native', () => {
    // Lane 24 retired the branch-option head: both consumers now delegate to
    // the native owner, and the helper is gone from this module.
    expect(helper).not.toContain('SELECT id,display_name name FROM organization_nodes');
    expect(pms).toContain('nativeLane24BranchOptions(facts.actor, facts.companyId, facts.actor.branchId)');
    expect(pos).toContain('nativeLane24PosBranchOptions(target, facts.company, facts.branch, facts.branch2)');
    expect(pms).not.toContain('branchNodeOptions');
    expect(pos).not.toContain('branchNodeOptions');
    expect(org).not.toContain(NODE_HEAD);
    expect(messaging).not.toContain(EXISTS_HEAD);
    expect(helper.split(NODE_HEAD).length - 1).toBe(1);
    expect(helper.split(EXISTS_HEAD).length - 1).toBe(1);

    for (const [call] of NODE_SITES) expect(org.split(call).length - 1).toBe(1);
    for (const [call] of MESSAGE_SITES) expect(messaging.split(call).length - 1).toBe(1);

    // Bind arrays stay byte-identical at every replaced call site.
    expect(pms).toContain('nativeLane24BranchOptions(facts.actor, facts.companyId, facts.actor.branchId)');
    expect(pos).toContain('nativeLane24PosBranchOptions(target, facts.company, facts.branch, facts.branch2)');
    expect(org).toContain('.bind(actor.companyId,actor.branchId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId)');
    expect(org).toContain('.bind(id,companyId).first<OrganizationNode>()');

    // FOR SHARE at pms-command-facts.ts:21 is untouched.
    expect(pms).toContain("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active' FOR SHARE");

    // LOCK TABLE, FOR SHARE/FOR UPDATE, native owner writes and spaced JOINs stay untouched.
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(org.split('nativeOrgStatement(').length - 1).toBe(15);
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');

    // r58 folds the two authority-seat EXISTS heads into the r57 renderer while
    // keeping their differing aliases and predicates as caller-supplied tails.
    // r55's guard33 person/engagement head is unchanged.
    const approvals = read('workflow-approvals-facts.ts');
    expect(createHash('sha256').update(approvals).digest('hex')).toBe(
      'c35091e4e00ce0ab13bfa08a5ec9d029f92f688656725b61f5ce8532d6f398f4',
    );
    expect(approvals.split('EXISTS(SELECT 1 FROM authority_seats').length - 1).toBe(0);
    expect(approvals.split('authoritySeatExistsSql(').length - 1).toBe(2);
    expect(approvals).toContain('authoritySeatExistsSql(`seat JOIN');
    expect(approvals).toContain('authoritySeatExistsSql(`s JOIN');
  });

  test('retirement:ts-statements is strictly lower than the r49 BEFORE=237 baseline (expected 196)', () => {
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
    // r49 collapses three pairs of read-tail SELECTs into three shared helpers (-3);
    // r58 folds the two approvals-facts authority-seat heads (-2): 211 - 2 = 209.
    // r59 folds two executive-metric count heads (-2 literals, +1 renderer): 209 - 1 = 208.
    // r60 folds two level-history head literals (-2 literals, +1 renderer): 208 - 1 = 207.
    // r61 folds two position-package projection heads (-2 literals, +1 renderer): 207 - 1 = 206.
    expect(space).toBe(196);
    expect(space).toBeLessThan(237);
  });
});
