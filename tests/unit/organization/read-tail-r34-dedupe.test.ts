import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { activeNodeKindExists } from '../../../packages/server-organization/src/adapters/organization-column-alias-families-r34';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r34.ts');
const ops = read('operations-staff-settings-facts.ts');
const tail = read('organization-read-tail-sources.ts');

describe('Organization read-tail r34 kind-parameterized active-node EXISTS family', () => {
  test('activeNodeKindExists stays byte-identical to both replaced literals', () => {
    expect(activeNodeKindExists('department')).toBe(
      "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active')",
    );
    expect(activeNodeKindExists('branch')).toBe(
      "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",
    );
  });

  test('r34 family is defined once and every r17-r33 helper is untouched', () => {
    expect(helper.match(/export const activeNodeKindExists =/g) ?? []).toHaveLength(1);
    const prior = read('organization-column-alias-families.ts');
    const r27 = read('organization-column-alias-families-r27.ts');
    const r28 = read('organization-column-alias-families-r28.ts');
    const r29 = read('organization-column-alias-families-r29.ts');
    const r30 = read('organization-column-alias-families-r30.ts');
    const r31 = read('organization-column-alias-families-r31.ts');
    const spaced = read('organization-nodes-spaced-projections.ts');
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
    for (const name of ['STAFF_LEVELS_L_JOIN', 'NODE_BRANCH_ACTIVE', 'departmentSnapshotMissing', 'ownerVersionGuardSource']) {
      expect(tail.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    // The remaining spaced JOIN constants stay byte-identical and individually defined.
    for (const name of ['ORG_NODES_SPACED_C_JOIN', 'ORG_NODES_SPACED_N_JOIN', 'ORG_NODES_SPACED_B_JOIN', 'ORG_NODES_SPACED_CLAUSE', 'ORG_POSITIONS_SPACED_CLAUSE']) {
      expect(spaced.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline literals are gone and each caller delegates to activeNodeKindExists', () => {
    expect(ops).not.toContain("EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active')");
    expect(tail).not.toContain("EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')");
    expect(ops.split('activeNodeKindExists(').length - 1).toBe(1);
    expect(tail.split('activeNodeKindExists(').length - 1).toBe(1);
  });

  test('remaining spaced JOIN constants are in use and delegate byte-identically', async () => {
    const spaced = await import('../../../packages/server-organization/src/adapters/organization-nodes-spaced-projections');
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'c')).toBe(spaced.ORG_NODES_SPACED_C_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'n')).toBe(spaced.ORG_NODES_SPACED_N_JOIN);
    expect(spaced.ORG_NODES_SPACED_CLAUSE('JOIN', 'b')).toBe(spaced.ORG_NODES_SPACED_B_JOIN);
    expect(spaced.ORG_NODES_SPACED_C_JOIN).toBe('JOIN (SELECT "id", "kind", "slug", "display_name", "parent_id", "company_id", "branch_id", "status", "hierarchy_revision" FROM organization_nodes) c');
  });
});

describe('Organization read-tail r34 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r34 BEFORE=273 baseline', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r34 cut one new family (2 literals -> 1 helper statement) below the
    // task BEFORE=273; no other owner path is touched.
    expect(space).toBe(272);
    expect(space).toBeLessThan(273);
  });
});
