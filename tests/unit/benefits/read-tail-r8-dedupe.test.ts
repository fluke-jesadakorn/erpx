import { describe, expect, test } from 'vitest';
import { readFileSync } from 'node:fs';
import { benefitScopePrefix, benefitWindowFactPrefix, benefitDependentFactPrefix } from '../../../packages/server-benefits/src/adapters/benefits-read-sql';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-benefits/src/adapters/${name}`, import.meta.url), 'utf8');

describe('Benefits read-tail r8 byte-identical dedupe', () => {
  test('scope CTE prefix is the exact shared literal and is read-only', () => {
    expect(benefitScopePrefix).toBe('WITH scope AS(SELECT ?::text company_id,');
    expect(benefitScopePrefix).toContain('SELECT ');
    expect(benefitScopePrefix).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('scope CTE prefix has one owner literal and two composed callers', () => {
    const owners = ['package-obligations.ts', 'people-package-obligations.ts'];
    for (const name of owners) {
      const text = src(name);
      expect(text, name).not.toContain(benefitScopePrefix);
      expect(text.match(/benefitScopePrefix/g) ?? [], name).toHaveLength(2);
    }
    expect(src('benefits-read-sql.ts').split(benefitScopePrefix)).toHaveLength(2);
  });

  test('composed SQL stays byte-identical to the pre-dedupe literal', () => {
    const pkg = src('package-obligations.ts');
    const people = src('people-package-obligations.ts');
    expect(pkg).toContain('${benefitScopePrefix}to_char(to_timestamp(?::bigint/1000.0)');
    expect(people).toContain('${benefitScopePrefix}?::boolean is_base,to_char(to_timestamp(?::bigint/1000.0)');
    const pkgSql = `${benefitScopePrefix}to_char(to_timestamp(?::bigint/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD') today)`;
    const peopleSql = `${benefitScopePrefix}?::boolean is_base,to_char(to_timestamp(?::bigint/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD') today)`;
    expect(pkgSql).toBe("WITH scope AS(SELECT ?::text company_id,to_char(to_timestamp(?::bigint/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD') today)");
    expect(peopleSql).toBe("WITH scope AS(SELECT ?::text company_id,?::boolean is_base,to_char(to_timestamp(?::bigint/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD') today)");
  });
});

describe('Benefits read-tail r9 byte-identical dedupe', () => {
  test('window fact prefix is the exact shared literal and is read-only', () => {
    expect(benefitWindowFactPrefix).toBe("UNION ALL SELECT 'window',w.id,w.version,w.status,to_jsonb(w),");
    expect(benefitWindowFactPrefix).toContain('SELECT ');
    expect(benefitWindowFactPrefix).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('window fact prefix has one owner literal and two composed callers', () => {
    const owners = ['package-obligations.ts', 'people-package-obligations.ts'];
    for (const name of owners) {
      const text = src(name);
      expect(text, name).not.toContain(benefitWindowFactPrefix);
      expect(text.match(/benefitWindowFactPrefix/g) ?? [], name).toHaveLength(2);
    }
    expect(src('benefits-read-sql.ts').split(benefitWindowFactPrefix)).toHaveLength(2);
  });

  test('composed SQL stays byte-identical to the pre-dedupe literal', () => {
    const pkg = src('package-obligations.ts');
    const people = src('people-package-obligations.ts');
    expect(pkg).toContain('${benefitWindowFactPrefix}w.status NOT IN');
    expect(people).toContain('${benefitWindowFactPrefix}w.status IN');
    const pkgSql = `${benefitWindowFactPrefix}w.status NOT IN ('closed','cancelled') OR NOT w.terminal_proven FROM windows w`;
    const peopleSql = `${benefitWindowFactPrefix}w.status IN ('draft','open') FROM benefit_enrollment_windows w,scope WHERE w.company_id=scope.company_id`;
    expect(pkgSql).toBe("UNION ALL SELECT 'window',w.id,w.version,w.status,to_jsonb(w),w.status NOT IN ('closed','cancelled') OR NOT w.terminal_proven FROM windows w");
    expect(peopleSql).toBe("UNION ALL SELECT 'window',w.id,w.version,w.status,to_jsonb(w),w.status IN ('draft','open') FROM benefit_enrollment_windows w,scope WHERE w.company_id=scope.company_id");
  });
});

describe('Benefits read-tail r10 byte-identical dedupe', () => {
  test('dependent fact prefix is the exact shared literal and is read-only', () => {
    expect(benefitDependentFactPrefix).toBe("UNION ALL SELECT 'dependent',d.id,d.version,d.status,to_jsonb(d),d.status='pending'");
    expect(benefitDependentFactPrefix).toContain('SELECT ');
    expect(benefitDependentFactPrefix).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('dependent fact prefix has one owner literal and two composed callers', () => {
    const owners = ['package-obligations.ts', 'people-package-obligations.ts'];
    for (const name of owners) {
      const text = src(name);
      expect(text, name).not.toContain(benefitDependentFactPrefix);
      expect(text.match(/benefitDependentFactPrefix/g) ?? [], name).toHaveLength(2);
    }
    expect(src('benefits-read-sql.ts').split(benefitDependentFactPrefix)).toHaveLength(2);
  });

  test('composed SQL stays byte-identical to the pre-dedupe literal', () => {
    const pkg = src('package-obligations.ts');
    const people = src('people-package-obligations.ts');
    expect(pkg).toContain('${benefitDependentFactPrefix} OR NOT d.terminal_proven FROM');
    expect(people).toContain('${benefitDependentFactPrefix} OR NOT EXISTS(SELECT 1 FROM');
    const pkgSql = `${benefitDependentFactPrefix} OR NOT d.terminal_proven FROM (${'${benefitPackageSettlementSources.dependents}'}) d,scope WHERE d.company_id=scope.company_id`;
    const peopleSql = `${benefitDependentFactPrefix} OR NOT EXISTS(SELECT 1 FROM (${'${people.people}'}) p WHERE p.id=d.person_id AND p.company_id=d.company_id) FROM benefit_dependents d,scope WHERE d.company_id=scope.company_id`;
    expect(pkgSql).toBe("UNION ALL SELECT 'dependent',d.id,d.version,d.status,to_jsonb(d),d.status='pending' OR NOT d.terminal_proven FROM (${benefitPackageSettlementSources.dependents}) d,scope WHERE d.company_id=scope.company_id");
    expect(peopleSql).toBe("UNION ALL SELECT 'dependent',d.id,d.version,d.status,to_jsonb(d),d.status='pending' OR NOT EXISTS(SELECT 1 FROM (${people.people}) p WHERE p.id=d.person_id AND p.company_id=d.company_id) FROM benefit_dependents d,scope WHERE d.company_id=scope.company_id");
  });
});
