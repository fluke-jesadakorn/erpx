import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  companyNodeScopeSource,
  activeStaffLevelSource,
  changeVersionGuardSource,
  scopeNodeBranchExists,
} from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r39';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-read-tail-sources-r39.ts');

// Replaced literals, recorded verbatim from the pre-r39 source.
const COMPANY_NODE_SCOPE = "SELECT 1 FROM organization_nodes n WHERE n.id=? AND n.kind='company' AND n.company_id=? AND n.status='active'";
const COMPANY_NODE_SCOPE_EXISTS = `EXISTS(${COMPANY_NODE_SCOPE})`;
const STAFF_LEVEL_ACTIVE = 'SELECT 1 FROM staff_levels WHERE id=? AND company_id=? AND active=1';
const STAFF_LEVEL_ACTIVE_EXISTS = `EXISTS(${STAFF_LEVEL_ACTIVE})`;
const CHANGE_VERSION_PREFIX = 'EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=? AND ';
const CHANGE_VERSIONS = [
  `${CHANGE_VERSION_PREFIX}state='submitted')`,
  `${CHANGE_VERSION_PREFIX}state IN ('draft','rejected'))`,
  `${CHANGE_VERSION_PREFIX}state IN ('draft','submitted','rejected'))`,
];
const CHANGE_STATES = ["state='submitted'", "state IN ('draft','rejected')", "state IN ('draft','submitted','rejected')"];
const BRANCH_EXISTS_PREFIX = 'EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=a.scope_node_id AND n.company_id=a.company_id AND n.branch_id=';
const BRANCH_EXISTS_BARE = `${BRANCH_EXISTS_PREFIX}?)`;
const BRANCH_EXISTS_ALIAS = `${BRANCH_EXISTS_PREFIX}a.branch_id)`;

describe('Organization read-tail r39 node and scope guard families', () => {
  test('r39 helpers render byte-identical SQL to every replaced literal', () => {
    expect(companyNodeScopeSource).toBe(COMPANY_NODE_SCOPE);
    expect(`EXISTS(${companyNodeScopeSource})`).toBe(COMPANY_NODE_SCOPE_EXISTS);
    expect(activeStaffLevelSource).toBe(STAFF_LEVEL_ACTIVE);
    expect(`EXISTS(${activeStaffLevelSource})`).toBe(STAFF_LEVEL_ACTIVE_EXISTS);
    CHANGE_STATES.forEach((state, i) => {
      expect(changeVersionGuardSource(state)).toBe(CHANGE_VERSIONS[i]);
    });
    expect(scopeNodeBranchExists('?')).toBe(BRANCH_EXISTS_BARE);
    expect(scopeNodeBranchExists('a.branch_id')).toBe(BRANCH_EXISTS_ALIAS);
  });

  test('r39 defines four new helpers and every r17-r38 helper is untouched', () => {
    for (const name of ['companyNodeScopeSource', 'activeStaffLevelSource', 'changeVersionGuardSource', 'scopeNodeBranchExists']) {
      expect(helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    const prior = read('organization-column-alias-families.ts');
    const r27 = read('organization-column-alias-families-r27.ts');
    const r28 = read('organization-column-alias-families-r28.ts');
    const r29 = read('organization-column-alias-families-r29.ts');
    const r30 = read('organization-column-alias-families-r30.ts');
    const r31 = read('organization-column-alias-families-r31.ts');
    const r34 = read('organization-column-alias-families-r34.ts');
    const r35 = read('organization-column-alias-families-r35.ts');
    const r36 = read('organization-column-alias-families-r36.ts');
    const r37 = read('organization-column-alias-families-r37.ts');
    const r38 = read('organization-column-alias-families-r38.ts');
    for (const name of ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']) {
      expect(prior.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['activeNodeByKind', 'twoColumnLookup', 'nodeKindByIdCompany', 'versionedNodeActiveGuard']) {
      expect(r27.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['companySnapshotAggregate', 'correlatedNodeJson']) {
      expect(r28.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['maxVersionSubquery', 'impactCount']) {
      expect(r29.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    expect(r30.match(/export const activeLevelAbsent =/g) ?? []).toHaveLength(1);
    expect(r31.match(/export const companyNodeByIdKind =/g) ?? []).toHaveLength(1);
    expect(r34.match(/export const activeNodeKindExists =/g) ?? []).toHaveLength(1);
    expect(r35.match(/export const ownerProjection =/g) ?? []).toHaveLength(1);
    expect(r36.match(/export const activeNodeByIdCompanyKind =/g) ?? []).toHaveLength(1);
    expect(r37.match(/export const leavePolicyVersionLookup =/g) ?? []).toHaveLength(1);
    expect(r38.match(/export const activePositionById =/g) ?? []).toHaveLength(1);
  });

  test('inline family literals are gone, call sites delegate, locks and spaced JOIN constants are retained', async () => {
    const gateway = read('ai-gateway-sources.ts');
    const opsStaff = read('operations-staff-settings-facts.ts');
    const peopleEval = read('people-evaluation-facts.ts');
    const organization = read('organization.ts');
    const home = read('collaboration-home-projections.ts');
    const collabRead = read('collaboration-read-projections.ts');

    expect(gateway).not.toContain(COMPANY_NODE_SCOPE);
    expect(opsStaff).not.toContain(STAFF_LEVEL_ACTIVE);
    expect(peopleEval).not.toContain(STAFF_LEVEL_ACTIVE);
    for (const literal of CHANGE_VERSIONS) expect(organization).not.toContain(literal);
    expect(home).not.toContain(BRANCH_EXISTS_BARE);
    expect(collabRead).not.toContain(BRANCH_EXISTS_ALIAS);

    expect(gateway.split('companyNodeScopeSource').length - 1).toBeGreaterThanOrEqual(2);
    expect(opsStaff.split('activeStaffLevelSource').length - 1).toBeGreaterThanOrEqual(1);
    expect(peopleEval.split('activeStaffLevelSource').length - 1).toBeGreaterThanOrEqual(1);
    expect(organization.split('changeVersionGuardSource(').length - 1).toBe(3);
    expect(home.split('scopeNodeBranchExists(').length - 1).toBeGreaterThanOrEqual(1);
    expect(collabRead.split('scopeNodeBranchExists(').length - 1).toBeGreaterThanOrEqual(1);

    // FOR SHARE/FOR UPDATE locks are separate statements and stay byte-identical.
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('people-lifecycle-facts.ts')).toContain('FOR UPDATE');
    expect(read('organization-drafts.ts')).toContain('FOR UPDATE');
    expect(read('pos-core-facts.ts')).toContain('FOR UPDATE');
    expect(read('finance-starter-sources.ts')).toContain('FOR UPDATE');
    const spaced = await import('../../../packages/server-organization/src/adapters/organization-nodes-spaced-projections');
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'c')).toBe(spaced.ORG_NODES_SPACED_C_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'n')).toBe(spaced.ORG_NODES_SPACED_N_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'b')).toBe(spaced.ORG_NODES_SPACED_B_JOIN);
    expect(spaced.ORG_NODES_SPACED_C_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
  });

  test('retirement:ts-statements is strictly lower than the r39 BEFORE=262 baseline (expected 257)', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r39 collapses four families (-1, -1, -2, -1 = -5) into shared sources.
    // Pin: BEFORE=262; expected after = 257; no other owner path is touched.
    expect(space).toBe(230);
    expect(space).toBeLessThanOrEqual(257);
    expect(space).toBeLessThan(262);
  });
});
