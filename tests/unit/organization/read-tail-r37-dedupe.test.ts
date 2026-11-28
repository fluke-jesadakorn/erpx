import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { leavePolicyVersionLookup } from '../../../modules/organization/server/src/adapters/organization-column-alias-families-r37';

const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r37.ts');
const staff = read('staff-defaults.ts');
const worker = read('people-lifecycle-worker-effects.ts');

// The two inline literals differ only in placeholder style ($1/$2 vs ?).
const SOURCE = '${people_payrollPlatformSources.leave_policy_versions}';
const WORKER_LITERAL = 'SELECT d.policy_version_id,p.effective_from FROM level_leave_defaults d JOIN (${people_payrollPlatformSources.leave_policy_versions}) p ON p.id=d.policy_version_id WHERE d.company_id=$1 AND d.level_id=$2 ORDER BY d.version DESC LIMIT 1';
const STAFF_LITERAL = 'SELECT d.policy_version_id,p.effective_from FROM level_leave_defaults d JOIN (${people_payrollPlatformSources.leave_policy_versions}) p ON p.id=d.policy_version_id WHERE d.company_id=? AND d.level_id=? ORDER BY d.version DESC LIMIT 1';

describe('Organization read-tail r37 leave-policy version family', () => {
  test('leavePolicyVersionLookup renders byte-identical SQL to every replaced literal', () => {
    expect(leavePolicyVersionLookup(SOURCE, '$1', '$2')).toBe(WORKER_LITERAL);
    expect(leavePolicyVersionLookup(SOURCE, '?', '?')).toBe(STAFF_LITERAL);
  });

  test('r37 defines one new helper and every r17-r36 helper is untouched', () => {
    expect(helper.match(/export const leavePolicyVersionLookup =/g) ?? []).toHaveLength(1);
    const prior = read('organization-column-alias-families.ts');
    const r27 = read('organization-column-alias-families-r27.ts');
    const r28 = read('organization-column-alias-families-r28.ts');
    const r29 = read('organization-column-alias-families-r29.ts');
    const r30 = read('organization-column-alias-families-r30.ts');
    const r31 = read('organization-column-alias-families-r31.ts');
    const r34 = read('organization-column-alias-families-r34.ts');
    const r35 = read('organization-column-alias-families-r35.ts');
    const r36 = read('organization-column-alias-families-r36.ts');
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
  });

  test('inline family literals are gone, sources delegate, locks and spaced JOIN constants are retained', async () => {
    expect(worker).not.toContain(WORKER_LITERAL);
    expect(staff).not.toContain(STAFF_LITERAL);
    expect(worker.split('leavePolicyVersionLookup(').length - 1).toBeGreaterThanOrEqual(1);
    expect(staff.split('leavePolicyVersionLookup(').length - 1).toBeGreaterThanOrEqual(1);
    // No locks were added to this family; the PMS FOR SHARE variant is a
    // different statement and stays byte-identical.
    expect(worker).not.toContain('FOR SHARE');
    expect(staff).not.toContain('FOR SHARE');
    const pms = read('pms-command-facts.ts');
    expect(pms).toContain('FOR SHARE');
    const spaced = await import('../../../modules/organization/server/src/adapters/organization-nodes-spaced-projections');
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'c')).toBe(spaced.ORG_NODES_SPACED_C_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'n')).toBe(spaced.ORG_NODES_SPACED_N_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'b')).toBe(spaced.ORG_NODES_SPACED_B_JOIN);
    expect(spaced.ORG_NODES_SPACED_C_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
  });

  test('retirement:ts-statements is strictly below the r37 BEFORE=264 baseline', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r37 collapsed a new 2-literal family into 1 helper statement (net -1)
    // below the task BEFORE=264; no other owner path is touched.
    expect(space).toBeLessThanOrEqual(263);
    expect(space).toBeLessThan(264);
  });
});
