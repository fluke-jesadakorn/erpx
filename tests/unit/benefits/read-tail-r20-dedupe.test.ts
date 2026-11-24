import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitOwnerExistsGuard } from '../../../packages/server-benefits/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe literals, copied from HEAD, used only as the byte-identity oracle.
const preDedupe = {
  versionGuard: (table: string) => `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
  payrollGuard: `EXISTS(SELECT 1 FROM benefit_payroll_inputs WHERE id=? AND engagement_id=? AND status='released' AND employee_deduction_minor=? AND employer_cost_minor=?)`,
} as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r20 parameterized owner existence-guard family', () => {
  test('generator reproduces both pre-dedupe guard SELECTs byte-for-byte', () => {
    for (const table of ['benefit_plans', 'benefit_dependents', 'benefit_elections']) {
      expect(benefitOwnerExistsGuard(table, 'company_id=? AND version=?')).toBe(preDedupe.versionGuard(table));
    }
    expect(benefitOwnerExistsGuard('benefit_payroll_inputs', "engagement_id=? AND status='released' AND employee_deduction_minor=? AND employer_cost_minor=?")).toBe(preDedupe.payrollGuard);
    readOnly(preDedupe.versionGuard('benefit_plans'));
    readOnly(preDedupe.payrollGuard);
  });

  test('generator has one owner literal and two composed callers', () => {
    expect(src('benefits-read-sql.ts').match(/benefitOwnerExistsGuard/g) ?? []).toHaveLength(1);
    const sensitive = src('attachments-sensitive-facts.ts');
    expect(sensitive).not.toContain('EXISTS(SELECT 1 FROM ${table} WHERE id=?');
    expect(sensitive.match(/benefitOwnerExistsGuard\(/g) ?? []).toHaveLength(1);
    const payroll = src('payroll-source-facts.ts');
    expect(payroll).not.toContain("EXISTS(SELECT 1 FROM benefit_payroll_inputs WHERE id=?");
    expect(payroll.match(/benefitOwnerExistsGuard\(/g) ?? []).toHaveLength(1);
  });

  test('the generator family adds no write or lock statement', () => {
    for (const name of ['benefits-read-sql.ts', 'attachments-sensitive-facts.ts', 'payroll-source-facts.ts']) {
      expect(src(name), name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });
});

describe('Benefits read-tail r20 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r20 BEFORE=44 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(44);
  });
});
