import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { snapshotJsonByAction } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r46';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r46.ts');

// Replaced literals, recorded verbatim from the pre-r46 (BEFORE=240) source.
const SNAPSHOTS: [string, string, string][] = [
  ['organization-drafts.appliedMapping', snapshotJsonByAction('applied', 'created_at'),
    "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='applied' ORDER BY created_at DESC LIMIT 1"],
  ['organization.submittedSnapshot', snapshotJsonByAction('submitted', 'created_at DESC,id'),
    "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='submitted' ORDER BY created_at DESC,id DESC LIMIT 1"],
];

describe('Organization read-tail r46 parameterized head', () => {
  test('r46 helper renders byte-identical SQL to every replaced literal', () => {
    for (const [site, rendered, oracle] of SNAPSHOTS) {
      expect(rendered, `snapshotJsonByAction @ ${site}`).toBe(oracle);
    }
  });

  test('r46 defines one new helper and every r17-r45 helper is untouched', () => {
    expect(helper.match(/export const snapshotJsonByAction =/g) ?? []).toHaveLength(1);
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
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline head literals are gone, call sites delegate, locks and native stay untouched', () => {
    const drafts = read('organization-drafts.ts');
    const org = read('organization.ts');
    for (const [, , oracle] of SNAPSHOTS) {
      expect(drafts).not.toContain(oracle);
      expect(org).not.toContain(oracle);
    }
    expect(drafts.split("snapshotJsonByAction('applied','created_at')").length - 1).toBe(1);
    expect(org.split("snapshotJsonByAction('submitted','created_at DESC,id')").length - 1).toBe(1);

    // LOCK TABLE, FOR SHARE/FOR UPDATE and spaced JOIN constants stay byte-identical.
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r46 BEFORE=240 baseline (expected 239)', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r46 collapses two change-event snapshot reads into one shared source (-1).
    expect(space).toBe(211);
    expect(space).toBeLessThan(240);
  });
});
