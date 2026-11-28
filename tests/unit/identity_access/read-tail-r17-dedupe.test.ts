import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { grantProbeSql } from '../../../modules/identity-access/server/src/adapters/iam-probe-sql';

const srcRoot = resolve(__dirname, '../../../modules/identity-access/server/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sqlSrc = read('adapters/iam-probe-sql.ts');
const cancelSrc = read('adapters/payroll-cancellation-grant.ts');
const recoverSrc = read('adapters/payroll-recovery-grant.ts');

describe('Identity access read-tail r17 active-grant probe family', () => {
  test('renders byte-identical SQL to both pre-dedupe wrapper literals', () => {
    const grant = 'EXISTS(SELECT 1 FROM assignments a WHERE a.id=?)';
    expect(grantProbeSql(grant)).toBe(`SELECT 1 WHERE ${grant}`);
  });

  test('is one owner helper with two callers and no inline wrapper left behind', () => {
    expect((sqlSrc.match(/export const grantProbeSql =/g) ?? []).length).toBe(1);
    expect((cancelSrc.match(/\bgrantProbeSql\(/g) ?? []).length).toBe(1);
    expect((recoverSrc.match(/\bgrantProbeSql\(/g) ?? []).length).toBe(1);
    expect(cancelSrc).not.toContain('SELECT 1 WHERE ');
    expect(recoverSrc).not.toContain('SELECT 1 WHERE ');
  });

  test('introduces no write or lock, and leaves lock statements and earlier round helpers untouched', () => {
    const family = sqlSrc.slice(sqlSrc.indexOf('export const grantProbeSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // The duplicate FOR SHARE lock family stays inline in both callers.
    expect(cancelSrc).toContain("'SELECT id FROM assignments WHERE id=? FOR SHARE'");
    expect(read('adapters/workflow-command-authority.ts')).toContain('FOR SHARE');
    for (const h of [
      'roleAllowsSql', 'samePersonSql', 'samePersonQuerySql', 'assignmentActiveQuerySql',
      'memberForShareSql', 'protectedMigrationIdentitySql', 'activeRecipientsSql',
      'idNameByUserIdsSql', 'verifiedUserEmailSql', 'idNameEmailByUserIdsSql', 'publishedRoleSql',
    ]) expect((sqlSrc.match(new RegExp(`export const ${h} =`, 'g')) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r17 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r17 BEFORE=533 baseline', () => {
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
    expect(total).toBeLessThan(533);
  });
});
