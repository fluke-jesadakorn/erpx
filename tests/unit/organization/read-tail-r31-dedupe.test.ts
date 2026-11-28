import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { companyNodeByIdKind } from '../../../modules/organization/server/src/adapters/organization-column-alias-families-r31';

const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r31.ts');
const sources = read('org-sources.ts');
const engineering = read('engineering-company-source.ts');

describe('Organization read-tail r31 company-node-by-kind projection family', () => {
  test('companyNodeByIdKind stays byte-identical to both replaced literals', () => {
    expect(companyNodeByIdKind('1')).toBe(
      "SELECT 1 FROM organization_nodes WHERE id=? AND kind='company'",
    );
    expect(companyNodeByIdKind('display_name')).toBe(
      "SELECT display_name FROM organization_nodes WHERE id=? AND kind='company'",
    );
  });

  test('r31 family is defined once and every r17-r30 helper is untouched', () => {
    expect(helper.match(/export const companyNodeByIdKind =/g) ?? []).toHaveLength(1);
    const prior = read('organization-column-alias-families.ts');
    const r27 = read('organization-column-alias-families-r27.ts');
    const r28 = read('organization-column-alias-families-r28.ts');
    const r29 = read('organization-column-alias-families-r29.ts');
    const r30 = read('organization-column-alias-families-r30.ts');
    const tail = read('organization-read-tail-sources.ts');
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
    for (const name of ['departmentSnapshotMissing', 'ownerVersionGuardSource']) {
      expect(tail.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline literals are gone and each caller uses companyNodeByIdKind', () => {
    expect(sources).not.toContain("SELECT display_name FROM organization_nodes WHERE id=? AND kind='company'");
    expect(engineering).not.toContain("SELECT 1 FROM organization_nodes WHERE id=? AND kind='company'");
    expect(sources.split('companyNodeByIdKind(').length - 1).toBe(1);
    expect(engineering.split('companyNodeByIdKind(').length - 1).toBe(1);
  });
});

describe('Organization read-tail r31 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r31 BEFORE=280 baseline', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r31 cut one new family (2 literals -> 1 helper statement) below the
    // canonical r30-after 280; the task's BEFORE baseline is 280. Later
    // rounds cut further families, so use a ceiling like the r27-r29 tests.
    expect(space).toBeLessThanOrEqual(277);
    expect(space).toBeLessThan(280);
  });
});
