import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  activeNodeByKind,
  twoColumnLookup,
  nodeKindByIdCompany,
  versionedNodeActiveGuard,
} from '../../../modules/organization/server/src/adapters/organization-column-alias-families-r27';

const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families-r27.ts');
const financeIntercompany = read('finance-intercompany-sources.ts');
const attachmentMigration = read('attachment-migration-owner.ts');
const posDaily = read('pos-daily-facts.ts');
const staffSettings = read('operations-staff-settings-facts.ts');
const peopleDirectory = read('people-directory-facts.ts');
const attachmentService = read('attachments-service-facts.ts');
const orgSources = read('org-sources.ts');
const productCompany = read('product-company.ts');

describe('Organization read-tail r27 column/kind-parameterized families', () => {
  test('activeNodeByKind stays byte-identical to the finance and legacy-brand literals', () => {
    expect(activeNodeByKind('id', 'company')).toBe(
      "SELECT id FROM organization_nodes WHERE id=? AND kind='company' AND status='active'",
    );
    expect(activeNodeByKind('company_id', 'company')).toBe(
      "SELECT company_id FROM organization_nodes WHERE id=? AND kind='company' AND status='active'",
    );
    // r54 point reads: the branch two-column projection, the canonical branch
    // constant, the company display-name projection and the holding-brand EXISTS.
    expect(activeNodeByKind('company_id,id branch_id', 'branch')).toBe(
      "SELECT company_id,id branch_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'",
    );
    expect(activeNodeByKind('company_id', 'branch')).toBe(
      "SELECT company_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'",
    );
    expect(activeNodeByKind('id,display_name AS name', 'company')).toBe(
      "SELECT id,display_name AS name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'",
    );
    expect(`?=1 AND EXISTS(${activeNodeByKind('1', 'company')})`).toBe(
      "?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND kind='company' AND status='active')",
    );
  });

  test('twoColumnLookup stays byte-identical to the level and roster literals', () => {
    expect(twoColumnLookup('level_number', 'staff_levels', 'id', 'company_id')).toBe(
      'SELECT level_number FROM staff_levels WHERE id=? AND company_id=?',
    );
    expect(twoColumnLookup('roster_required', 'organization_positions', 'company_id', 'id')).toBe(
      'SELECT roster_required FROM organization_positions WHERE company_id=? AND id=?',
    );
  });

  test('nodeKindByIdCompany stays byte-identical to the POS and department-head literals', () => {
    expect(nodeKindByIdCompany('id', 'branch')).toBe(
      "SELECT id FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch'",
    );
    expect(nodeKindByIdCompany('branch_id', 'department')).toBe(
      "SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=? AND kind='department'",
    );
  });

  test('versionedNodeActiveGuard stays byte-identical to both attachment owner guards', () => {
    expect(versionedNodeActiveGuard('company_id', 'branch')).toBe(
      "?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",
    );
    expect(versionedNodeActiveGuard('id', 'company')).toBe(
      "?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND id=? AND kind='company' AND status='active')",
    );
  });

  test('each r27 family is defined exactly once and r17-r26 helpers are untouched', () => {
    for (const name of ['activeNodeByKind', 'twoColumnLookup', 'nodeKindByIdCompany', 'versionedNodeActiveGuard']) {
      expect(helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
    const prior = read('organization-column-alias-families.ts');
    for (const name of ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']) {
      expect(prior.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline literals are gone and every fact reader calls its r27 helper', () => {
    expect(financeIntercompany).not.toContain("SELECT id FROM organization_nodes WHERE id=? AND kind='company' AND status='active'");
    expect(financeIntercompany.split('activeNodeByKind(').length - 1).toBe(1);
    expect(attachmentMigration).not.toContain("SELECT company_id FROM organization_nodes WHERE id=? AND kind='company' AND status='active'");
    expect(attachmentMigration).not.toContain("SELECT company_id,id branch_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'");
    expect(attachmentMigration.split('activeNodeByKind(').length - 1).toBe(2);
    expect(attachmentMigration).toContain("activeNodeByKind('company_id,id branch_id','branch')");
    expect(attachmentMigration).toContain('.bind(id).first<{company_id:string;branch_id:string}>()');
    expect(orgSources).not.toContain("SELECT company_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'");
    expect(orgSources.split('activeNodeByKind(').length - 1).toBe(1);
    expect(orgSources).toContain("activeNodeByKind('company_id','branch');");
    expect(orgSources).toContain("ORG_BRANCH_COMPANY_SOURCE =\n  activeNodeByKind('company_id','branch');");
    expect(productCompany).not.toContain("SELECT id,display_name AS name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'");
    expect(productCompany.split('activeNodeByKind(').length - 1).toBe(1);
    expect(productCompany).toContain(".bind(companyId).first<{ id: string; name: string }>()");
    expect(posDaily).not.toContain("SELECT id FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch'");
    expect(posDaily.split('nodeKindByIdCompany(').length - 1).toBe(1);
    expect(staffSettings).not.toContain('SELECT level_number FROM staff_levels WHERE id=? AND company_id=?');
    expect(staffSettings).not.toContain("SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=? AND kind='department'");
    expect(staffSettings.split('twoColumnLookup(').length - 1).toBe(1);
    expect(staffSettings.split('nodeKindByIdCompany(').length - 1).toBe(1);
    expect(peopleDirectory).not.toContain('SELECT roster_required FROM organization_positions WHERE company_id=? AND id=?');
    expect(peopleDirectory.split('twoColumnLookup(').length - 1).toBe(1);
    expect(attachmentService).not.toContain("?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')");
    expect(attachmentService).not.toContain("?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND id=? AND kind='company' AND status='active')");
    expect(attachmentService.split('versionedNodeActiveGuard(').length - 1).toBe(2);
    expect(attachmentService).not.toContain("?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND kind='company' AND status='active')");
    expect(attachmentService.split('activeNodeByKind(').length - 1).toBe(1);
    expect(attachmentService).toContain("`?=1 AND EXISTS(${activeNodeByKind('1','company')})`, [version, companyId]");
  });
});

describe('Organization read-tail r27 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r27 BEFORE=303 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r28 cut two further families below the r27 count; family assertions above are unchanged.
    expect(total).toBeLessThanOrEqual(277);
    expect(total).toBeLessThan(303);
  });
});
