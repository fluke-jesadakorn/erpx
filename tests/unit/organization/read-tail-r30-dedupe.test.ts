import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { activeLevelAbsent } from '../../../packages/server-organization/src/adapters/organization-column-alias-families-r30';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r30.ts');
const payroll = read('payroll-owner-facts.ts');
const directory = read('people-directory-facts.ts');

// Fixed value of platformOwnerProjectionSources.hr_engagements (the same source
// the call sites pass). Kept local so the helper stays free of server-only imports.
const HR_ENGAGEMENTS = 'SELECT "id","person_id","company_id","branch_id","department_id","worker_type","employment_type","status","start_date","end_date","manager_person_id","job_title","currency","version","created_by","created_at","updated_at","pay_group_id","level_id","position_id","self_service_top_level" FROM hr_engagements';

describe('Organization read-tail r30 active-level engagement guard family', () => {
  test('activeLevelAbsent stays byte-identical to the three guard literals', () => {
    expect(activeLevelAbsent(HR_ENGAGEMENTS, "e.id=? AND e.company_id=? AND e.status='active' AND (l.id IS NULL OR l.active!=1)")).toBe(
      `SELECT 1 FROM (${HR_ENGAGEMENTS}) e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.id=? AND e.company_id=? AND e.status='active' AND (l.id IS NULL OR l.active!=1)`,
    );
    expect(`NOT EXISTS(${activeLevelAbsent(HR_ENGAGEMENTS, "e.id=? AND e.status='active' AND (l.id IS NULL OR l.active!=1)")})`).toBe(
      `NOT EXISTS(SELECT 1 FROM (${HR_ENGAGEMENTS}) e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.id=? AND e.status='active' AND (l.id IS NULL OR l.active!=1))`,
    );
    expect(`NOT EXISTS(${activeLevelAbsent(HR_ENGAGEMENTS, "e.person_id=? AND e.status!='ended' AND (e.level_id IS NULL OR l.active IS DISTINCT FROM 1)")})`).toBe(
      `NOT EXISTS(SELECT 1 FROM (${HR_ENGAGEMENTS}) e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.person_id=? AND e.status!='ended' AND (e.level_id IS NULL OR l.active IS DISTINCT FROM 1))`,
    );
  });

  test('r30 family is defined once and every r17-r29 helper is untouched', () => {
    expect(helper.match(/export const activeLevelAbsent =/g) ?? []).toHaveLength(1);
    const prior = read('organization-column-alias-families.ts');
    const r27 = read('organization-column-alias-families-r27.ts');
    const r28 = read('organization-column-alias-families-r28.ts');
    const r29 = read('organization-column-alias-families-r29.ts');
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
    for (const name of ['departmentSnapshotMissing', 'ownerVersionGuardSource']) {
      expect(tail.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline literals are gone and each guard calls activeLevelAbsent', () => {
    const inline = 'SELECT 1 FROM (${people_payrollPlatformSources.hr_engagements}) e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.';
    expect(payroll).not.toContain(inline);
    expect(directory).not.toContain(inline);
    expect(payroll.split('activeLevelAbsent(').length - 1).toBe(2);
    expect(directory.split('activeLevelAbsent(').length - 1).toBe(1);
  });
});

describe('Organization read-tail r30 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r30 BEFORE=282 baseline', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r30 cut one new family (3 literals -> 1 helper statement) below the
    // canonical r29-after 282; the task's BEFORE baseline is 282. r31 cut one
    // further new family, so the live count is now 279.
    expect(space).toBe(279);
    expect(space).toBeLessThan(282);
  });
});
