import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { activeNodeKindList } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r43';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r43.ts');

// Replaced literals, recorded verbatim from the pre-r43 (BEFORE=244) source.
const KIND_LIST: [string, string, string][] = [
  [
    'project-worker-startup-sources.ts',
    activeNodeKindList("'branch','headquarters','department'", ' ORDER BY kind,id'),
    "SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN ('branch','headquarters','department') ORDER BY kind,id",
  ],
  [
    'records-request-page-facts.ts',
    activeNodeKindList("'company','branch','department'", " AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR kind='branch' OR id=?) ORDER BY display_name"),
    "SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR kind='branch' OR id=?) ORDER BY display_name",
  ],
  [
    'people-directory-facts.ts',
    activeNodeKindList("'branch','department'", " AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR kind='branch' OR id=?) ORDER BY display_name"),
    "SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN ('branch','department') AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR kind='branch' OR id=?) ORDER BY display_name",
  ],
];

describe('Organization read-tail r43 byte-identical family', () => {
  test('r43 helper renders byte-identical SQL to every replaced literal', () => {
    for (const [site, rendered, oracle] of KIND_LIST) {
      expect(rendered, `activeNodeKindList @ ${site}`).toBe(oracle);
    }
  });

  test('r43 defines one new helper and every r17-r42 helper is untouched', () => {
    expect(helper.match(/export const activeNodeKindList =/g) ?? []).toHaveLength(1);
    for (const [file, names] of [
      ['organization-column-alias-families.ts', ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']],
      ['organization-column-alias-families-r27.ts', ['activeNodeByKind', 'twoColumnLookup', 'nodeKindByIdCompany', 'versionedNodeActiveGuard']],
      ['organization-column-alias-families-r28.ts', ['companySnapshotAggregate', 'correlatedNodeJson']],
      ['organization-read-tail-sources.ts', ['departmentSnapshotMissing', 'ownerVersionGuardSource', 'NODE_BRANCH_ACTIVE', 'NODE_BRANCH_ACTIVE_EXISTS']],
      ['organization-read-tail-sources-r39.ts', ['companyNodeScopeSource', 'activeStaffLevelSource', 'changeVersionGuardSource', 'scopeNodeBranchExists']],
      ['organization-read-tail-sources-r40.ts', ['activePositionList', 'recommendedPayDefault', 'nodeChangeByIdCompany', 'activeStaffLevels', 'memberProfileByUser']],
      ['organization-read-tail-sources-r41.ts', ['relocationVersionSnapshot']],
      ['organization-read-tail-sources-r42.ts', ['activeCompanyNodes', 'branchOptionalId', 'latestDepartmentFlow', 'organizationChangeRows']],
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline family literals are gone, call sites delegate, locks and spaced JOIN constants are retained', () => {
    const head = 'SELECT id,kind,display_name,branch_id FROM organization_nodes';
    const pw = read('project-worker-startup-sources.ts');
    const rp = read('records-request-page-facts.ts');
    const pd = read('people-directory-facts.ts');
    expect(pw).not.toContain(head);
    expect(rp).not.toContain(head);
    expect(pd).not.toContain(head);
    expect(pw).toContain("activeNodeKindList(\"'branch','headquarters','department'\", ' ORDER BY kind,id')");
    expect(rp).toContain("activeNodeKindList(\"'company','branch','department'\",");
    expect(pd).toContain("activeNodeKindList(\"'branch','department'\",");

    // LOCK TABLE, FOR SHARE/FOR UPDATE and spaced JOIN constants stay byte-identical.
    expect(pw).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r43 BEFORE=244 baseline (expected 242)', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r43 collapses three active-node-kind option reads into one shared source (-2).
    expect(space).toBe(207);
    expect(space).toBeLessThan(244);
  });
});
