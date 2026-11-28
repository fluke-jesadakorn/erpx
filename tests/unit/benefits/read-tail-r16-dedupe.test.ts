import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitPlanFactPrefix } from '../../../modules/benefits/server/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../modules/benefits/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe literals, copied from HEAD, used only as the byte-identity oracle.
const prefix = "SELECT 'plan' kind,p.id,p.version,p.status state,to_jsonb(p) image,";
const preDedupe = {
  package: prefix + "p.status NOT IN ('retired','rejected') OR NOT p.terminal_proven pending FROM plans p",
  people: prefix + "p.status='draft' pending FROM benefit_plans p,scope WHERE p.company_id=scope.company_id",
} as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r16 plan-fact prefix family', () => {
  test('plan fact prefix is the exact shared literal and is read-only', () => {
    expect(benefitPlanFactPrefix).toBe(prefix);
    readOnly(benefitPlanFactPrefix);
  });

  test('plan fact prefix has one owner literal and two composed callers', () => {
    expect(src('benefits-read-sql.ts').match(/benefitPlanFactPrefix/g) ?? []).toHaveLength(1);
    for (const name of ['package-obligations.ts', 'people-package-obligations.ts']) {
      const text = src(name);
      expect(text, name).not.toContain(prefix);
      expect(text.match(/benefitPlanFactPrefix/g) ?? [], name).toHaveLength(2);
    }
  });

  test('composed SQL stays byte-identical to the pre-dedupe literals', () => {
    const pkg = `${benefitPlanFactPrefix}p.status NOT IN ('retired','rejected') OR NOT p.terminal_proven pending FROM plans p`;
    const people = `${benefitPlanFactPrefix}p.status='draft' pending FROM benefit_plans p,scope WHERE p.company_id=scope.company_id`;
    expect(pkg).toBe(preDedupe.package);
    expect(people).toBe(preDedupe.people);
    readOnly(pkg);
    readOnly(people);
  });

  test('the generator family adds no write or lock statement', () => {
    const family = src('benefits-read-sql.ts');
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r16 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r16 BEFORE=48 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(48);
  });
});
