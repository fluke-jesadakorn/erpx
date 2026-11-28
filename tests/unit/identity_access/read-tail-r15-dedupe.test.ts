import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { idNameEmailByUserIdsSql } from '../../../modules/identity-access/server/src/adapters/iam-probe-sql';

const srcRoot = resolve(__dirname, '../../../modules/identity-access/server/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sqlSrc = read('adapters/iam-probe-sql.ts');
const ids = ['u1', 'u2'];

describe('Identity access read-tail r15 id/name/email family', () => {
  test('renders byte-identical SQL to the pre-dedupe literals at both callers', () => {
    expect(idNameEmailByUserIdsSql(ids)).toBe('SELECT id,name,email FROM "user" WHERE id IN (?,?)');
  });

  test('is one owner literal with two callers and no inline literal left behind', () => {
    expect((sqlSrc.match(/export const idNameEmailByUserIdsSql =/g) ?? []).length).toBe(1);
    const callers = [
      'adapters/iam-service-reads.ts',
      'adapters/member-directory.ts',
    ];
    let uses = 0;
    for (const rel of callers) {
      const s = read(rel);
      uses += (s.match(/\bidNameEmailByUserIdsSql\(/g) ?? []).length;
      expect(s).not.toContain('SELECT id,name,email FROM "user" WHERE id IN (');
    }
    expect(uses).toBe(2);
  });

  test('introduces no write or lock and leaves r13/r14 helpers untouched', () => {
    const family = sqlSrc.slice(sqlSrc.indexOf('export const idNameEmailByUserIdsSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect((sqlSrc.match(/export const activeRecipientsSql =/g) ?? []).length).toBe(1);
    expect((sqlSrc.match(/export const idNameByUserIdsSql =/g) ?? []).length).toBe(1);
    expect((sqlSrc.match(/export const verifiedUserEmailSql/g) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r15 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r15 BEFORE=535 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) {
          total += (readFileSync(p, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
        }
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(535);
  });
});
