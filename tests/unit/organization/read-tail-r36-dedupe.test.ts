import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { activeNodeByIdCompanyKind } from '../../../packages/server-organization/src/adapters/organization-column-alias-families-r36';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r36.ts');
const tail = read('organization-read-tail-sources.ts');
const front = read('front-company-facts.ts');
const pms = read('pms-command-facts.ts');

const BRANCH = "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'";
const COMPANY = "SELECT slug FROM organization_nodes WHERE id=? AND company_id=? AND kind='company' AND status='active'";

describe('Organization read-tail r36 active-node-by-kind family', () => {
  test('activeNodeByIdCompanyKind renders byte-identical SQL to every replaced literal', () => {
    expect(activeNodeByIdCompanyKind('1', 'branch')).toBe(BRANCH);
    expect(activeNodeByIdCompanyKind('slug', 'company')).toBe(COMPANY);
  });

  test('r36 defines one new helper and every r17-r35 helper is untouched', () => {
    expect(helper.match(/export const activeNodeByIdCompanyKind =/g) ?? []).toHaveLength(1);
    const prior = read('organization-column-alias-families.ts');
    const r27 = read('organization-column-alias-families-r27.ts');
    const r28 = read('organization-column-alias-families-r28.ts');
    const r29 = read('organization-column-alias-families-r29.ts');
    const r30 = read('organization-column-alias-families-r30.ts');
    const r31 = read('organization-column-alias-families-r31.ts');
    const r34 = read('organization-column-alias-families-r34.ts');
    const r35 = read('organization-column-alias-families-r35.ts');
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
  });

  test('inline family literals are gone, sources delegate, FOR SHARE lock variant is untouched', async () => {
    expect(tail).not.toContain(BRANCH);
    expect(front).not.toContain(COMPANY);
    // The r36 helper renders the same SQL plus no lock; the PMS FOR SHARE
    // variant is a different statement and must remain byte-identical.
    expect(pms).toContain(`${BRANCH} FOR SHARE`);
    expect(tail.split('activeNodeByIdCompanyKind(').length - 1).toBeGreaterThanOrEqual(1);
    expect(front.split('activeNodeByIdCompanyKind(').length - 1).toBeGreaterThanOrEqual(1);
    const { NODE_BRANCH_ACTIVE } = await import('../../../packages/server-organization/src/adapters/organization-read-tail-sources');
    expect(NODE_BRANCH_ACTIVE).toBe(activeNodeByIdCompanyKind('1', 'branch'));
  });

  test('spaced JOIN constants and native-read-tail guarantees are retained', async () => {
    const spaced = await import('../../../packages/server-organization/src/adapters/organization-nodes-spaced-projections');
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'c')).toBe(spaced.ORG_NODES_SPACED_C_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'n')).toBe(spaced.ORG_NODES_SPACED_N_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'b')).toBe(spaced.ORG_NODES_SPACED_B_JOIN);
    expect(spaced.ORG_NODES_SPACED_C_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
    expect(pms).toContain('FOR SHARE');
    expect(tail).not.toContain('UPDATE ');
    expect(front).not.toContain('UPDATE ');
  });
});

describe('Organization read-tail r36 retirement check', () => {
  test('retirement:ts-statements is strictly below the r36 BEFORE=265 baseline', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r36 collapsed a new 2-literal family into 1 helper statement (net -1)
    // below the task BEFORE=265; no other owner path is touched.
    expect(space).toBeLessThanOrEqual(264);
    expect(space).toBeLessThan(265);
  });
});
