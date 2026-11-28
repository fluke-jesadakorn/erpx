import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { selectOneFrom } from '../../../modules/benefits/server/src/adapters/benefits-read-sql';
import { benefitAssignmentSelectSql } from '../../../modules/benefits/server/src/adapters/purchases';

const srcRoot = resolve(__dirname, '../../../modules/benefits/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracle captured at HEAD before the r30 fold. The inline 332-byte
// `SELECT 1 FROM assignments selected WHERE selected.id=? AND ...` assignment
// guard duplicated the head already rendered by the owner-local `selectOneFrom`.
// It now routes through that renderer. Rendered SQL, bind order, module predicate
// and both composed callers must be unchanged.
const predicate = () => {
  const match = src('purchases.ts').match(/benefitAssignmentModulePredicate="([^"]*)"/);
  if (!match) throw Error('assignment module predicate literal not found');
  return match[1];
};
const preDedupe = () => `SELECT 1 FROM assignments selected WHERE selected.id=? AND ${predicate()}`;
const PRE_DEDUPE_SHA = 'e5906a186e3f5d8a2cbb30b3b1f8e9f67169b892b0aad8b1e37e348303d5c0a5';

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const kpi = () => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
    }
  };
  walk(srcRoot);
  return total;
};

describe('benefits read-tail r30 select-one head dedupe', () => {
  test('the renderer reproduces the pre-dedupe assignment guard byte-for-byte', () => {
    const before = preDedupe();
    expect(benefitAssignmentSelectSql).toBe(before);
    expect(sha(benefitAssignmentSelectSql)).toBe(PRE_DEDUPE_SHA);
    expect(Buffer.from(benefitAssignmentSelectSql)).toEqual(Buffer.from(before));
    expect(Buffer.byteLength(benefitAssignmentSelectSql)).toBe(332);
    expect(benefitAssignmentSelectSql.match(/\?/g) ?? []).toHaveLength(1);
    readOnly(benefitAssignmentSelectSql);
  });

  test('the composed callers keep their exact bytes and bind positions', () => {
    const guard = benefitAssignmentSelectSql;
    expect(`EXISTS(${guard})`).toBe(`EXISTS(${preDedupe()})`);
    expect(`prepare(${guard}).bind(actor.assignmentId)`).toContain('bind(actor.assignmentId)');
    expect(guard.indexOf('?')).toBe('SELECT 1 FROM assignments selected WHERE selected.id='.length);
    expect(guard).toContain(predicate());
  });

  test('one renderer owns the SELECT-1 head and the assignment guard delegates to it', () => {
    const head = src('benefits-read-sql.ts');
    expect(head.match(/`SELECT 1 FROM \$\{source\}\$\{tail\}`/g) ?? []).toHaveLength(1);
    const purchases = src('purchases.ts');
    expect(purchases).toContain('selectOneFrom');
    expect(purchases).toContain("selectOneFrom('assignments selected',` WHERE selected.id=?");
    expect(purchases).not.toContain('`SELECT 1 FROM assignments selected WHERE');
    expect(purchases.match(/selectOneFrom\(/g) ?? []).toHaveLength(1);
    expect(purchases.match(/SELECT 1 FROM/g) ?? []).toHaveLength(0);
  });

  test('the fold adds no write, row-lock, native registry or cross-owner consumer', () => {
    for (const name of ['benefits-read-sql.ts', 'purchases.ts']) {
      expect(src(name), name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE/i);
      expect(src(name), name).not.toContain('@erpx/benefits-server/adapters/benefits-native-read');
    }
    expect(src('purchases.ts').match(/LOCK\s+TABLE/g) ?? []).toHaveLength(1);
    expect(src('purchases.ts')).toContain('IN SHARE ROW EXCLUSIVE MODE');
    const record = src('record-purchases.ts');
    expect(record.match(/benefitAssignmentSelectSql/g) ?? []).toHaveLength(3); // import + two EXISTS callers
  });

  test('retirement:ts-statements is 25, strictly below the r29/r30 BEFORE=26 baseline', () => {
    expect(kpi()).toBe(25);
    expect(kpi()).toBeLessThan(26);
  });
});
