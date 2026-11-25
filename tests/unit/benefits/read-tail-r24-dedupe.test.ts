import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  benefitCompanyScopeHead,
  benefitScopePrefix,
  benefitSourceProbe,
  benefitSubqueryNotExists,
  workforceActorScopeProbe,
} from '../../../packages/server-benefits/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe literals, copied byte-for-byte from the r24 base HEAD, used only
// as oracles. Generated SQL and binds must remain unchanged after composition.
const PRED_C = 'c.election_id=e.id AND c.company_id=e.company_id AND c.certificate_id=e.learning_certificate_id AND c.person_id=e.person_id AND c.eligibility_code=plan.required_eligibility_code';
const PRED_P = 'p.id=d.person_id AND p.company_id=d.company_id';
const preDedupeNotExists = (source: string, alias: string, predicate: string) =>
  `NOT EXISTS(SELECT 1 FROM (${source}) ${alias} WHERE ${predicate})`;
const preDedupeWorkforce = 'SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id';
const preDedupeScope = 'WITH scope AS(SELECT ?::text company_id,';
const preDedupeHead = 'SELECT ?::text company_id,';

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r24 subquery-probe composition', () => {
  test('benefitSubqueryNotExists composes benefitSourceProbe byte-for-byte', () => {
    for (const source of ['(learning.consumptions)', '(people.people)', 'benefit_source']) {
      expect(benefitSourceProbe(source, 'c')).toBe(`SELECT 1 FROM (${source}) c`);
      expect(benefitSubqueryNotExists(source, 'c', PRED_C)).toBe(preDedupeNotExists(source, 'c', PRED_C));
      expect(benefitSubqueryNotExists(source, 'p', PRED_P)).toBe(preDedupeNotExists(source, 'p', PRED_P));
      readOnly(preDedupeNotExists(source, 'c', PRED_C));
      readOnly(preDedupeNotExists(source, 'p', PRED_P));
    }
  });

  test('one owner definition of each builder; composed callers unchanged', () => {
    const sql = src('benefits-read-sql.ts');
    expect(sql.match(/export const benefitSubqueryNotExists\s*=/g) ?? []).toHaveLength(1);
    expect(sql.match(/export const benefitSourceProbe\s*=/g) ?? []).toHaveLength(1);
    expect(sql).not.toContain('NOT EXISTS(SELECT 1 FROM (${source})');
    for (const name of ['package-obligations.ts', 'people-package-obligations.ts']) {
      const text = src(name);
      expect(text.match(/benefitSubqueryNotExists\(/g) ?? []).toHaveLength(1);
    }
  });

  test('the composition adds no write or lock statement', () => {
    expect(src('benefits-read-sql.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(src('package-obligations.ts').match(/LOCK\s+TABLE/g) ?? []).toHaveLength(1);
    expect(src('people-package-obligations.ts').match(/LOCK\s+TABLE/g) ?? []).toHaveLength(1);
  });
});

describe('Benefits read-tail r24 scope-head extraction', () => {
  test('one head constant renders both probes byte-for-byte', () => {
    expect(benefitCompanyScopeHead).toBe(preDedupeHead);
    expect(workforceActorScopeProbe).toBe(preDedupeWorkforce);
    expect(benefitScopePrefix).toBe(preDedupeScope);
    readOnly(preDedupeWorkforce);
    readOnly(preDedupeScope);
  });

  test('one owner definition and two composed consumers; raw heads removed', () => {
    const sql = src('benefits-read-sql.ts');
    expect(sql.match(/export const benefitCompanyScopeHead\s*=/g) ?? []).toHaveLength(1);
    expect(sql.match(/benefitCompanyScopeHead/g) ?? []).toHaveLength(3);
    expect(sql).not.toContain(preDedupeWorkforce);
    expect(sql).not.toContain(preDedupeScope);
    for (const name of ['attachment-purchases.ts', 'attachment-read.ts', 'record-purchases.ts']) {
      expect(src(name), name).toContain('workforceActorScopeProbe');
    }
    for (const name of ['package-obligations.ts', 'people-package-obligations.ts']) {
      expect(src(name), name).toContain('${benefitScopePrefix}');
    }
  });

  test('the extraction adds no write or lock statement', () => {
    expect(src('benefits-read-sql.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r24 retirement check', () => {
  test('retirement:ts-statements is 35, strictly below the r24 BEFORE=37 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(35);
    expect(total).toBeLessThan(37);
  });
});
