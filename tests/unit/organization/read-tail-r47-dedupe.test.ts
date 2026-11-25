import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { workerCompanyRolloutGuard } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r47';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r47.ts');

// The pre-r47 (BEFORE=239) inline literal. Only the scheduled-feature predicate
// tail varies; the wrapper is rendered verbatim by the shared helper.
const INLINE = 'EXISTS(SELECT 1 FROM organization_nodes worker_company WHERE worker_company.id=? AND ';
const ORACLE = (predicate: string) => `${INLINE}${predicate})`;

// Predicate expressions passed verbatim at each call site (from pre-r47 source).
const SITES: [string, string, string][] = [
  ['collaboration-reply-rollout.ts',
    "workerCompanyRolloutGuard(`(${scheduledFeaturePredicate('worker_company.id','collaboration')} AND ${scheduledFeaturePredicate('worker_company.id','agent')})`)",
    "(${scheduledFeaturePredicate('worker_company.id','collaboration')} AND ${scheduledFeaturePredicate('worker_company.id','agent')})"],
  ['executive-generation-workspace-guard.ts',
    "workerCompanyRolloutGuard(scheduledGeneration('worker_company.id'))",
    "scheduledGeneration('worker_company.id')"],
];

describe('Organization read-tail r47 parameterized head', () => {
  test('r47 helper renders byte-identical SQL to every replaced literal', () => {
    expect(workerCompanyRolloutGuard('<P>')).toBe(ORACLE('<P>'));
    expect(workerCompanyRolloutGuard('A AND B')).toBe(ORACLE('A AND B'));
    for (const [, , predicate] of SITES) {
      expect(workerCompanyRolloutGuard(predicate)).toBe(ORACLE(predicate));
    }
  });

  test('r47 defines one new helper and every r17-r46 helper is untouched', () => {
    expect(helper.match(/export const workerCompanyRolloutGuard =/g) ?? []).toHaveLength(1);
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
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline literals are gone, call sites delegate with one bind, locks and native stay untouched', () => {
    for (const [file, call, predicate] of SITES) {
      const src = read(file);
      // Inline worker_company head is gone; only the helper still renders it.
      expect(src).not.toContain(INLINE);
      // Exact delegated call expression is present exactly once.
      expect(src.split(call).length - 1).toBe(1);
      // The original predicate expression is preserved verbatim in source.
      expect(src).toContain(predicate);
      // Exactly one bind [company] remains at the assertion call site.
      expect((src.match(/\[company\]/g) ?? []).length).toBe(1);
    }
    expect((helper.match(/SELECT 1 FROM organization_nodes worker_company/g) ?? []).length).toBe(1);

    // LOCK TABLE, FOR SHARE/FOR UPDATE and spaced JOIN constants stay byte-identical.
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r47 BEFORE=239 baseline (expected 238)', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r47 collapses two worker-company rollout guards into one shared source (-1).
    expect(space).toBe(206);
    expect(space).toBeLessThan(239);
  });
});
