import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  companyIdProjection,
  nodeTreeCte,
} from '../../../packages/server-organization/src/adapters/organization-column-alias-families';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const helper = readFileSync(resolve(adapters, 'organization-column-alias-families.ts'), 'utf8');
const directoryFacts = readFileSync(resolve(adapters, 'directory-projection-facts.ts'), 'utf8');
const rosterFacts = readFileSync(resolve(adapters, 'people-roster-facts.ts'), 'utf8');

describe('Organization read-tail r25 column/alias-parameterized families', () => {
  test('companyIdProjection stays byte-identical to the four pre-dedupe literals', () => {
    expect(companyIdProjection('id', 'organization_nodes')).toBe(
      'SELECT id AS "companyId" FROM organization_nodes',
    );
    expect(companyIdProjection('p.company_id', 'organization_positions p')).toBe(
      'SELECT p.company_id AS "companyId" FROM organization_positions p',
    );
    expect(companyIdProjection('l.company_id', 'staff_levels l')).toBe(
      'SELECT l.company_id AS "companyId" FROM staff_levels l',
    );
    expect(companyIdProjection('n.company_id', 'organization_nodes n')).toBe(
      'SELECT n.company_id AS "companyId" FROM organization_nodes n',
    );
  });

  test('all companyId members share one rendered form after alias/column normalization', () => {
    const members = [
      companyIdProjection('id', 'organization_nodes'),
      companyIdProjection('p.company_id', 'organization_positions p'),
      companyIdProjection('l.company_id', 'staff_levels l'),
      companyIdProjection('n.company_id', 'organization_nodes n'),
    ];
    const normalize = (s: string) =>
      s.replace(/^SELECT (?:[a-z]+\.)?[a-z_]+ AS/, 'SELECT <COL> AS').replace(/ FROM .+$/, ' FROM <FROM>');
    expect(new Set(members.map(normalize)).size).toBe(1);
  });

  test('nodeTreeCte stays byte-identical to the subtree and ancestors CTE literals', () => {
    expect(nodeTreeCte('subtree', 'id', 'n.id', 'p', 'n.parent_id=p.id')).toBe(
      `WITH RECURSIVE subtree AS (\n    SELECT id FROM organization_nodes WHERE id=? AND company_id=?\n    UNION ALL SELECT n.id FROM organization_nodes n JOIN subtree p ON n.parent_id=p.id WHERE n.company_id=? AND n.status='active')`,
    );
    expect(nodeTreeCte('ancestors', 'id,parent_id', 'n.id,n.parent_id', 'c', 'n.id=c.parent_id')).toBe(
      `WITH RECURSIVE ancestors AS (\n    SELECT id,parent_id FROM organization_nodes WHERE id=? AND company_id=?\n    UNION ALL SELECT n.id,n.parent_id FROM organization_nodes n JOIN ancestors c ON n.id=c.parent_id WHERE n.company_id=? AND n.status='active')`,
    );
  });

  test('each family is defined exactly once in the shared helper', () => {
    expect(helper.match(/export const companyIdProjection =/g) ?? []).toHaveLength(1);
    expect(helper.match(/export const nodeTreeCte =/g) ?? []).toHaveLength(1);
  });

  test('inline literals are gone and both fact readers call their helper', () => {
    expect(directoryFacts).not.toContain('AS "companyId" FROM');
    expect(directoryFacts.split('companyIdProjection(').length - 1).toBe(4);
    expect(rosterFacts).not.toContain('WITH RECURSIVE subtree AS (');
    expect(rosterFacts).not.toContain('WITH RECURSIVE ancestors AS (');
    expect(rosterFacts.split('nodeTreeCte(').length - 1).toBe(2);
  });
});

describe('Organization read-tail r25 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r25 BEFORE=310 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(310);
  });
});
