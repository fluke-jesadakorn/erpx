import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { nodeSlugByKind } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r44';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r44.ts');

// Replaced literals, recorded verbatim from the pre-r44 (BEFORE=242) source.
const SLUGS: [string, string, string][] = [
  ['front.branch', nodeSlugByKind('branch', ''),
    "SELECT slug FROM organization_nodes WHERE company_id=? AND kind='branch' AND slug=? AND status='active'"],
  ['front.department', nodeSlugByKind('department', ' AND branch_id IS NULL'),
    "SELECT slug FROM organization_nodes WHERE company_id=? AND kind='department' AND slug=? AND branch_id IS NULL AND status='active'"],
];

describe('Organization read-tail r44 byte-identical family', () => {
  test('r44 helper renders byte-identical SQL to every replaced literal', () => {
    for (const [site, rendered, oracle] of SLUGS) {
      expect(rendered, `nodeSlugByKind @ ${site}`).toBe(oracle);
    }
  });

  test('r44 defines one new helper and every r17-r43 helper is untouched', () => {
    expect(helper.match(/export const nodeSlugByKind =/g) ?? []).toHaveLength(1);
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
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline family literals are gone, call sites delegate, locks and native stay untouched', () => {
    const front = read('front-company-facts.ts');
    for (const [, , oracle] of SLUGS) expect(front).not.toContain(oracle);
    expect(front.split("nodeSlugByKind('branch', '')").length - 1).toBe(1);
    expect(front.split("nodeSlugByKind('department', ' AND branch_id IS NULL')").length - 1).toBe(1);
    // The r36 slug-by-id helper in the same file is still delegated to.
    expect(front.split('activeNodeByIdCompanyKind(').length - 1).toBeGreaterThanOrEqual(1);

    // LOCK TABLE, FOR SHARE/FOR UPDATE and spaced JOIN constants stay byte-identical.
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r44 BEFORE=242 baseline (expected 241)', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r44 collapses two front-company slug reads into one shared source (-1).
    expect(space).toBe(230);
    expect(space).toBeLessThan(242);
  });
});
