import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitFactFinalSelect } from '../../../modules/benefits/server/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../modules/benefits/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe literals, copied from HEAD, used only as the byte-identity oracle.
const preDedupe = {
  package: ') SELECT kind,id,version,state,image,COALESCE(pending,true) pending FROM facts ORDER BY kind,id',
  people: ') SELECT kind,id,version,state,image,scope.is_base AND COALESCE(facts.pending,true) pending FROM facts CROSS JOIN scope ORDER BY kind,id',
} as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r18 parameterized fact-final projection family', () => {
  test('generator reproduces both pre-dedupe final SELECTs byte-for-byte', () => {
    expect(benefitFactFinalSelect('COALESCE(pending,true)', ' ORDER BY kind,id')).toBe(preDedupe.package);
    expect(benefitFactFinalSelect('scope.is_base AND COALESCE(facts.pending,true)', ' CROSS JOIN scope ORDER BY kind,id')).toBe(preDedupe.people);
    readOnly(preDedupe.package);
    readOnly(preDedupe.people);
  });

  test('generator has one owner literal and two composed callers', () => {
    expect(src('benefits-read-sql.ts').match(/benefitFactFinalSelect/g) ?? []).toHaveLength(1);
    for (const name of ['package-obligations.ts', 'people-package-obligations.ts']) {
      const text = src(name);
      expect(text, name).not.toContain('SELECT kind,id,version,state,image,');
      expect(text.match(/benefitFactFinalSelect\(/g) ?? [], name).toHaveLength(1);
    }
  });

  test('the generator family adds no write or lock statement', () => {
    expect(src('benefits-read-sql.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r18 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r18 BEFORE=46 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(46);
  });
});
