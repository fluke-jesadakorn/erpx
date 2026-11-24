import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { ecommerceModuleAssignment, ecommerceScopeRow } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const pb = readFileSync(resolve(adapters, 'purchase-boundary.ts'), 'utf8');
const ap = readFileSync(resolve(adapters, 'attachment-purchases.ts'), 'utf8');

// Pre-dedupe literals copied from HEAD, kept only as byte-identity oracles.
const head = {
  moduleBare: `SELECT 1 FROM assignments WHERE id=? AND company_id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> '["ecommerce"]'::jsonb)`,
  moduleAlias: `SELECT 1 FROM assignments assignment WHERE assignment.id=? AND assignment.company_id=? AND (jsonb_array_length(assignment.modules_json::jsonb)=0 OR assignment.modules_json::jsonb @> '["ecommerce"]'::jsonb)`,
  scopeCompany: (alias: string) => `(SELECT ?::text company_id) ${alias}`,
  scopeBranch: (alias: string) => `(SELECT ?::text company_id,?::text branch_id,?::text department_id) ${alias}`,
};

describe('Ecommerce read-tail r5 parameterized families', () => {
  test('assignment module-check is byte-identical with and without alias', () => {
    expect(ecommerceModuleAssignment()).toBe(head.moduleBare);
    expect(ecommerceModuleAssignment('assignment')).toBe(head.moduleAlias);
  });

  test('scope-row subquery is byte-identical for every caller alias', () => {
    expect(ecommerceScopeRow('ecommerce_release')).toBe(head.scopeCompany('ecommerce_release'));
    expect(ecommerceScopeRow('ecommerce_file')).toBe(head.scopeCompany('ecommerce_file'));
    expect(ecommerceScopeRow('ecommerce_file_release')).toBe(head.scopeCompany('ecommerce_file_release'));
    expect(ecommerceScopeRow('ecommerce_file_review', true)).toBe(head.scopeBranch('ecommerce_file_review'));
    expect(ecommerceScopeRow('ecommerce_file_scope', true)).toBe(head.scopeBranch('ecommerce_file_scope'));
  });

  test('families are defined once and both callers compose from them', () => {
    expect(shared.match(/export const ecommerceModuleAssignment =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const ecommerceScopeRow =/g) ?? []).toHaveLength(1);
    expect(pb.match(/ecommerceModuleAssignment\(/g) ?? []).toHaveLength(1);
    expect(pb.match(/ecommerceScopeRow\(/g) ?? []).toHaveLength(1);
    expect(ap.match(/ecommerceModuleAssignment\(/g) ?? []).toHaveLength(2);
    expect(ap.match(/ecommerceScopeRow\(/g) ?? []).toHaveLength(5);
    expect(pb).not.toContain('SELECT 1 FROM assignments WHERE');
    expect(ap).not.toContain('SELECT 1 FROM assignments WHERE');
    expect(ap).not.toContain('(SELECT ?::text company_id,?::text branch_id,?::text department_id)');
  });

  test('composed fragments stay read-only and keep every bind token', () => {
    for (const sql of [
      ecommerceModuleAssignment(),
      ecommerceModuleAssignment('assignment'),
      ecommerceScopeRow('ecommerce_release'),
      ecommerceScopeRow('ecommerce_file_review', true),
    ]) {
      expect(sql).toMatch(/SELECT/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect((ecommerceModuleAssignment().match(/\?/g) ?? []).length).toBe(2);
    expect((ecommerceModuleAssignment('assignment').match(/\?/g) ?? []).length).toBe(2);
    expect((ecommerceScopeRow('ecommerce_file').match(/\?/g) ?? []).length).toBe(1);
    expect((ecommerceScopeRow('ecommerce_file', true).match(/\?/g) ?? []).length).toBe(3);
  });
});

describe('Ecommerce read-tail r5 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r5 BEFORE=74 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(74);
  });
});
