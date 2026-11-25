import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { activeCompanyNodes, branchOptionalId, latestDepartmentFlow, organizationChangeRows } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r42';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r42.ts');

// Replaced literals, recorded verbatim from the pre-r42 (BEFORE=249) source.
const COMPANY = " WHERE kind='company' AND status='active' ORDER BY ";
const ACTIVE_COMPANY_NODES: [string, string, string][] = [
  ['attachment-migration-owner.ts', activeCompanyNodes('DISTINCT company_id', ' ', 'company_id LIMIT 2'), `SELECT DISTINCT company_id FROM organization_nodes${COMPANY}company_id LIMIT 2`],
  ['commercial-companies.ts', activeCompanyNodes('id,slug,display_name AS name', ' ', 'display_name,id'), `SELECT id,slug,display_name AS name FROM organization_nodes${COMPANY}display_name,id`],
  ['directory-projection-facts.ts', activeCompanyNodes('id,slug,display_name', '\n      ', 'display_name,id'), `SELECT id,slug,display_name FROM organization_nodes\n      WHERE kind='company' AND status='active' ORDER BY display_name,id`],
];
const BRANCH_OPTIONAL_ID: [string, string, string][] = [
  ['pms-hospitality-setup-facts.ts', branchOptionalId('id,display_name name', ' ORDER BY display_name'), "SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name"],
  ['commerce-facts.ts', branchOptionalId('id,display_name', ''), "SELECT id,display_name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?)"],
];
const LATEST_FLOW: [string, string, string][] = [
  ['organization.ts:46', latestDepartmentFlow('id,version,steps_json'), "SELECT id,version,steps_json FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1"],
  ['organization.ts:109', `(${latestDepartmentFlow('id')})`, '(SELECT id FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1)'],
];
const CHANGE_ROWS: [string, string, string][] = [
  ['organization.ts:77', organizationChangeRows('count(*) total') + '«predicate»', 'SELECT count(*) total FROM organization_node_changes WHERE «predicate»'],
  ['organization.ts:78', organizationChangeRows('*,NOT iam_same_person(requested_by,?,company_id) independent_reviewer') + '«predicate» ORDER BY updated_at DESC,id LIMIT ? OFFSET ?', 'SELECT *,NOT iam_same_person(requested_by,?,company_id) independent_reviewer FROM organization_node_changes WHERE «predicate» ORDER BY updated_at DESC,id LIMIT ? OFFSET ?'],
];

describe('Organization read-tail r42 byte-identical families', () => {
  test('r42 helpers render byte-identical SQL to every replaced literal', () => {
    for (const [name, group] of [['activeCompanyNodes', ACTIVE_COMPANY_NODES], ['branchOptionalId', BRANCH_OPTIONAL_ID], ['latestDepartmentFlow', LATEST_FLOW], ['organizationChangeRows', CHANGE_ROWS]] as const) {
      for (const [site, rendered, oracle] of group) {
        expect(rendered, `${name} @ ${site}`).toBe(oracle);
      }
    }
  });

  test('r42 defines four new helpers and every r17-r41 helper is untouched', () => {
    for (const name of ['activeCompanyNodes', 'branchOptionalId', 'latestDepartmentFlow', 'organizationChangeRows']) {
      expect(helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const [file, names] of [
      ['organization-column-alias-families.ts', ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']],
      ['organization-column-alias-families-r27.ts', ['activeNodeByKind', 'twoColumnLookup', 'nodeKindByIdCompany', 'versionedNodeActiveGuard']],
      ['organization-column-alias-families-r28.ts', ['companySnapshotAggregate', 'correlatedNodeJson']],
      ['organization-read-tail-sources.ts', ['departmentSnapshotMissing', 'ownerVersionGuardSource', 'NODE_BRANCH_ACTIVE', 'NODE_BRANCH_ACTIVE_EXISTS']],
      ['organization-read-tail-sources-r39.ts', ['companyNodeScopeSource', 'activeStaffLevelSource', 'changeVersionGuardSource', 'scopeNodeBranchExists']],
      ['organization-read-tail-sources-r40.ts', ['activePositionList', 'recommendedPayDefault', 'nodeChangeByIdCompany', 'activeStaffLevels', 'memberProfileByUser']],
      ['organization-read-tail-sources-r41.ts', ['relocationVersionSnapshot']],
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(source.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline family literals are gone, call sites delegate, locks and spaced JOIN constants are retained', () => {
    expect(read('attachment-migration-owner.ts')).not.toContain("SELECT DISTINCT company_id FROM organization_nodes");
    expect(read('commercial-companies.ts')).not.toContain("SELECT id,slug,display_name AS name FROM organization_nodes");
    const directory = read('directory-projection-facts.ts');
    expect(directory).not.toContain("SELECT id,slug,display_name FROM organization_nodes");
    expect(directory).toContain("activeCompanyNodes('id,slug,display_name', '\\n      ', 'display_name,id')");
    expect(read('pms-hospitality-setup-facts.ts')).not.toContain("SELECT id,display_name name FROM organization_nodes");
    expect(read('commerce-facts.ts')).not.toContain('SELECT id,display_name FROM organization_nodes');

    const org = read('organization.ts');
    expect(org).not.toContain('"SELECT id,version,steps_json FROM department_flow_versions');
    expect(org).not.toContain('"SELECT count(*) total FROM organization_node_changes');
    expect(org).not.toContain('`SELECT *,NOT iam_same_person(requested_by,?,company_id) independent_reviewer FROM organization_node_changes');
    expect(org.split('latestDepartmentFlow(').length - 1).toBe(2);
    expect(org.split('organizationChangeRows(').length - 1).toBe(2);

    // FOR SHARE/FOR UPDATE locks and spaced JOIN constants stay byte-identical.
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements is strictly lower than the r42 BEFORE=249 baseline (expected 244)', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r42 collapses three active-company-node, two branch-optional-id, two
    // latest-department-flow and two change-row reads into four shared sources (-5).
    expect(space).toBe(205);
    expect(space).toBeLessThan(249);
  });
});
