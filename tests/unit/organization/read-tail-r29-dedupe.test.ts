import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  maxVersionSubquery,
  impactCount,
} from '../../../packages/server-organization/src/adapters/organization-column-alias-families-r29';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r29.ts');
const staffDefaults = read('staff-defaults.ts');
const lifecycle = read('people-lifecycle-facts.ts');
const organization = read('organization.ts');

describe('Organization read-tail r29 relation/predicate-parameterized families', () => {
  test('maxVersionSubquery stays byte-identical to the three version-subquery literals', () => {
    expect(maxVersionSubquery('level_leave_defaults', 'level_id')).toBe(
      '(SELECT max(x.version) FROM level_leave_defaults x WHERE x.company_id=d.company_id AND x.level_id=d.level_id)',
    );
    expect(maxVersionSubquery('position_pay_defaults', 'position_id')).toBe(
      '(SELECT max(x.version) FROM position_pay_defaults x WHERE x.company_id=d.company_id AND x.position_id=d.position_id)',
    );
  });

  test('impactCount stays byte-identical to the five node-impact count literals', () => {
    expect(impactCount('organization_nodes', "parent_id=? AND status='active'")).toBe(
      "(SELECT count(*) FROM organization_nodes WHERE company_id=? AND parent_id=? AND status='active')",
    );
    expect(impactCount('(${people_payrollPlatformSources.hr_engagements}) hr_engagements', "status='active' AND (branch_id=? OR department_id=?)")).toBe(
      "(SELECT count(*) FROM (${people_payrollPlatformSources.hr_engagements}) hr_engagements WHERE company_id=? AND status='active' AND (branch_id=? OR department_id=?))",
    );
    expect(impactCount('organization_positions', 'active=1 AND (branch_id=? OR department_id=?)')).toBe(
      '(SELECT count(*) FROM organization_positions WHERE company_id=? AND active=1 AND (branch_id=? OR department_id=?))',
    );
    expect(impactCount('(${identity_accessPlatformSources.assignments}) assignments', 'revoked_at IS NULL AND scope_node_id=?')).toBe(
      '(SELECT count(*) FROM (${identity_accessPlatformSources.assignments}) assignments WHERE company_id=? AND revoked_at IS NULL AND scope_node_id=?)',
    );
    expect(impactCount('(${commercePlatformSources.commerce_warehouses}) commerce_warehouses', 'branch_id=?')).toBe(
      '(SELECT count(*) FROM (${commercePlatformSources.commerce_warehouses}) commerce_warehouses WHERE company_id=? AND branch_id=?)',
    );
  });

  test('each r29 family is defined exactly once and r17-r28 helpers are untouched', () => {
    for (const name of ['maxVersionSubquery', 'impactCount']) {
      expect(helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    const prior = read('organization-column-alias-families.ts');
    const priorR27 = read('organization-column-alias-families-r27.ts');
    const priorR28 = read('organization-column-alias-families-r28.ts');
    const tail = read('organization-read-tail-sources.ts');
    for (const name of ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']) {
      expect(prior.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['activeNodeByKind', 'twoColumnLookup', 'nodeKindByIdCompany', 'versionedNodeActiveGuard']) {
      expect(priorR27.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['companySnapshotAggregate', 'correlatedNodeJson']) {
      expect(priorR28.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    for (const name of ['departmentSnapshotMissing', 'ownerVersionGuardSource']) {
      expect(tail.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline literals are gone and every fact reader calls its r29 helper', () => {
    expect(staffDefaults).not.toContain('(SELECT max(x.version) FROM level_leave_defaults x');
    expect(staffDefaults).not.toContain('(SELECT max(x.version) FROM position_pay_defaults x');
    expect(lifecycle).not.toContain('(SELECT max(x.version) FROM level_leave_defaults x');
    expect(staffDefaults.split('maxVersionSubquery(').length - 1).toBe(2);
    expect(lifecycle.split('maxVersionSubquery(').length - 1).toBe(1);
    for (const count of ['count(*) FROM organization_nodes', 'count(*) FROM organization_positions']) {
      expect(organization).not.toContain(`(SELECT ${count} WHERE company_id=?`);
    }
    expect(organization).not.toContain('(SELECT count(*) FROM (${people_payrollPlatformSources.hr_engagements})');
    expect(organization).not.toContain('(SELECT count(*) FROM (${identity_accessPlatformSources.assignments})');
    expect(organization).not.toContain('(SELECT count(*) FROM (${commercePlatformSources.commerce_warehouses})');
    expect(organization.split('impactCount(').length - 1).toBe(5);
  });
});

describe('Organization read-tail r29 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r29 BEFORE=288 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r29 cut two new families (8 literals -> 2 helper statements) below the
    // canonical r28-after 292; the task's space-suffixed BEFORE baseline is 288.
    expect(total).toBe(286);
    expect(total).toBeLessThan(288);
  });
});
