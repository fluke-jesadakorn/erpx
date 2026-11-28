import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  companyIdProjection,
  nodeTreeCte,
  nodeActiveScope,
  maxVersionSource,
  denialAggregateClause,
  staffLevelNumberSource,
  snapshotProjection,
} from '../../../modules/organization/server/src/adapters/organization-column-alias-families';

const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const helper = read('organization-column-alias-families.ts');
const legalScope = read('legal-people-scope.ts');
const collaborationHome = read('collaboration-home-sources.ts');
const staffDefaults = read('staff-defaults.ts');
const denial = read('private-file-denial-sources.ts');
const organization = read('organization.ts');

describe('Organization read-tail r26 column/alias-parameterized families', () => {
  test('nodeActiveScope stays byte-identical to the three pre-dedupe literals', () => {
    expect(nodeActiveScope('kind')).toBe(
      "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind=? AND status='active'",
    );
    expect(nodeActiveScope('branch_id')).toBe(
      "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND status='active'",
    );
    expect(`EXISTS(${nodeActiveScope('kind')})`).toBe(
      "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind=? AND status='active')",
    );
    const normalize = (s: string) => s.replace(/ AND (kind|branch_id)=\?/, ' AND <COL>=?');
    expect(new Set([nodeActiveScope('kind'), nodeActiveScope('branch_id')].map(normalize)).size).toBe(1);
  });

  test('maxVersionSource stays byte-identical to the pay/leave version literals', () => {
    expect(maxVersionSource('position_pay_defaults', 'position_id')).toBe(
      'SELECT coalesce(max(version),0) version FROM position_pay_defaults WHERE company_id=? AND position_id=?',
    );
    expect(maxVersionSource('level_leave_defaults', 'level_id')).toBe(
      'SELECT coalesce(max(version),0) version FROM level_leave_defaults WHERE company_id=? AND level_id=?',
    );
  });

  test('denialAggregateClause stays byte-identical to the node/position denial clauses', () => {
    expect(denialAggregateClause('organization_nodes', 'node')).toBe(
      '(SELECT jsonb_agg(to_jsonb(node) ORDER BY node.id) FROM organization_nodes node WHERE node.company_id=denial_scope.company_id)',
    );
    expect(denialAggregateClause('organization_positions', 'position')).toBe(
      '(SELECT jsonb_agg(to_jsonb(position) ORDER BY position.id) FROM organization_positions position WHERE position.company_id=denial_scope.company_id)',
    );
  });

  test('staffLevelNumberSource stays byte-identical to both predicate orders', () => {
    expect(staffLevelNumberSource('id', 'company_id')).toBe(
      'SELECT level_number FROM staff_levels WHERE id=? AND company_id=? AND active=1',
    );
    expect(staffLevelNumberSource('company_id', 'id')).toBe(
      'SELECT level_number FROM staff_levels WHERE company_id=? AND id=? AND active=1',
    );
  });

  test('snapshotProjection stays byte-identical to both two-column snapshot reads', () => {
    expect(snapshotProjection('version,person_id', 'department_heads', 'company_id', 'department_id')).toBe(
      'SELECT version,person_id FROM department_heads WHERE company_id=? AND department_id=?',
    );
    expect(snapshotProjection('branch_id,slug', 'organization_nodes', 'id', 'company_id')).toBe(
      'SELECT branch_id,slug FROM organization_nodes WHERE id=? AND company_id=?',
    );
  });

  test('every family is defined exactly once and r17-r25 exports stay untouched', () => {
    for (const name of ['companyIdProjection', 'nodeTreeCte', 'nodeActiveScope', 'maxVersionSource', 'denialAggregateClause', 'staffLevelNumberSource', 'snapshotProjection']) {
      expect(helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).toHaveLength(1);
    }
  });

  test('inline literals are gone and every fact reader calls its helper', () => {
    expect(legalScope).not.toContain("AND kind=? AND status='active'");
    expect(legalScope.split('nodeActiveScope(').length - 1).toBe(2);
    expect(collaborationHome).not.toContain("EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind=?");
    expect(collaborationHome.split('nodeActiveScope(').length - 1).toBe(1);
    expect(staffDefaults).not.toContain('SELECT coalesce(max(version),0) version FROM');
    expect(staffDefaults.split('maxVersionSource(').length - 1).toBe(2);
    expect(staffDefaults.split('staffLevelNumberSource(').length - 1).toBe(1);
    expect(denial).not.toContain('(SELECT jsonb_agg(to_jsonb(node) ORDER BY node.id)');
    expect(denial.split('denialAggregateClause(').length - 1).toBe(2);
    expect(organization).not.toContain("SELECT version,person_id FROM department_heads WHERE");
    expect(organization).not.toContain("SELECT branch_id,slug FROM organization_nodes WHERE");
    expect(organization).not.toContain("SELECT level_number FROM staff_levels WHERE company_id=? AND id=? AND active=1");
    expect(organization.split('snapshotProjection(').length - 1).toBe(2);
    expect(organization.split('staffLevelNumberSource(').length - 1).toBe(1);
  });
});

describe('Organization read-tail r26 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r26 BEFORE=305 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(305);
  });
});
