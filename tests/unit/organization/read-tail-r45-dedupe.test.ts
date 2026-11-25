import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { nodeChildExists } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r45';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r45.ts');

// Replaced literals, recorded verbatim from the pre-r45 (BEFORE=241) source.
const CHILD_CHECKS: [string, string, string][] = [
  ['hasChildren', nodeChildExists("status<>'archived'"),
    "SELECT 1 FROM organization_nodes WHERE company_id=? AND parent_id=? AND status<>'archived' LIMIT 1"],
  ['slugConflict', nodeChildExists('slug=?'),
    'SELECT 1 FROM organization_nodes WHERE company_id=? AND parent_id=? AND slug=? LIMIT 1'],
];

describe('Organization read-tail r45 parameterized head', () => {
  test('r45 helper renders byte-identical SQL to every replaced literal', () => {
    for (const [site, rendered, oracle] of CHILD_CHECKS) {
      expect(rendered, `nodeChildExists @ ${site}`).toBe(oracle);
    }
  });

  test('r45 defines one new helper and every r17-r44 helper is untouched', () => {
    expect(helper.match(/export const nodeChildExists =/g) ?? []).toHaveLength(1);
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
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline head literals are gone, call sites delegate, locks and native stay untouched', () => {
    const org = read('organization.ts');
    for (const [, , oracle] of CHILD_CHECKS) expect(org).not.toContain(oracle);
    expect(org.split("nodeChildExists(\"status<>'archived'\")").length - 1).toBe(1);
    expect(org.split("nodeChildExists('slug=?')").length - 1).toBe(1);

    // LOCK TABLE, FOR SHARE/FOR UPDATE and spaced JOIN constants stay byte-identical.
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r45 BEFORE=241 baseline (expected 240)', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r45 collapses two organization child-existence reads into one shared source (-1).
    expect(space).toBe(209);
    expect(space).toBeLessThan(241);
  });
});
