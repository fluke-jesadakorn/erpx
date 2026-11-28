import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitSubqueryNotExists } from '../../../modules/benefits/server/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../modules/benefits/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe literals, copied byte-for-byte from HEAD, used only as the oracle.
// $SOURCE stands for the caller-supplied subquery text (learning.consumptions /
// people.people); the wrapper and predicate are the byte-identity subject.
const PRED_C = 'c.election_id=e.id AND c.company_id=e.company_id AND c.certificate_id=e.learning_certificate_id AND c.person_id=e.person_id AND c.eligibility_code=plan.required_eligibility_code';
const PRED_P = 'p.id=d.person_id AND p.company_id=d.company_id';
const preDedupe = {
  learning: (source: string) => `NOT EXISTS(SELECT 1 FROM (${source}) c WHERE ${PRED_C})`,
  people: (source: string) => `NOT EXISTS(SELECT 1 FROM (${source}) p WHERE ${PRED_P})`,
} as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r22 parameterized subquery absence-probe family', () => {
  test('generator reproduces both pre-dedupe probe SELECTs byte-for-byte', () => {
    for (const source of ['(learning.consumptions)', '(people.people)', 'benefit_source']) {
      expect(benefitSubqueryNotExists(source, 'c', PRED_C)).toBe(preDedupe.learning(source));
      expect(benefitSubqueryNotExists(source, 'p', PRED_P)).toBe(preDedupe.people(source));
      readOnly(preDedupe.learning(source));
      readOnly(preDedupe.people(source));
    }
  });

  test('generator has one owner literal and one composed caller in each consumer', () => {
    expect(src('benefits-read-sql.ts').match(/benefitSubqueryNotExists/g) ?? []).toHaveLength(1);
    const benefits = src('package-obligations.ts');
    expect(benefits).not.toContain('NOT EXISTS(SELECT 1 FROM (${learning.consumptions})');
    expect(benefits.match(/benefitSubqueryNotExists\(/g) ?? []).toHaveLength(1);
    const people = src('people-package-obligations.ts');
    expect(people).not.toContain('NOT EXISTS(SELECT 1 FROM (${people.people})');
    expect(people.match(/benefitSubqueryNotExists\(/g) ?? []).toHaveLength(1);
  });

  test('the new family adds no write or lock statement', () => {
    expect(src('benefits-read-sql.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // Pre-existing source locks in the two consumers are untouched (one each).
    expect(src('package-obligations.ts').match(/LOCK\s+TABLE/g) ?? []).toHaveLength(1);
    expect(src('people-package-obligations.ts').match(/LOCK\s+TABLE/g) ?? []).toHaveLength(1);
  });
});

describe('Benefits read-tail r22 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r22 BEFORE=42 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(42);
  });
});
