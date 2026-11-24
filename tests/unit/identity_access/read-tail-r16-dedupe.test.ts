import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { publishedRoleSql } from '../../../packages/server-identity_access/src/adapters/iam-probe-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sqlSrc = read('adapters/iam-probe-sql.ts');
const proj = 'permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v';
const storeOrig = `SELECT r.*,v.${proj} ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',?) AND r.id=? AND r.status='active'`;
const pgOrig = `SELECT r.*,v.${proj} ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.status='active'`;

describe('Identity access read-tail r16 published-role family', () => {
  test('renders byte-identical SQL for both qmark and pg positional dialects', () => {
    expect(publishedRoleSql('?', '?')).toBe(storeOrig);
    expect(publishedRoleSql('$1', '$2')).toBe(pgOrig);
  });

  test('is one owner helper with two callers and no inline literal left behind', () => {
    expect((sqlSrc.match(/export const publishedRoleSql =/g) ?? []).length).toBe(1);
    const callers = [
      'adapters/iam-store.ts',
      'adapters/iam-grant-transaction.ts',
    ];
    let uses = 0;
    for (const rel of callers) {
      const s = read(rel);
      uses += (s.match(/\bpublishedRoleSql\(/g) ?? []).length;
      expect(s).not.toContain(`SELECT r.*,v.${proj}`);
    }
    expect(uses).toBe(2);
  });

  test('introduces no write or lock and leaves earlier round helpers untouched', () => {
    const family = sqlSrc.slice(sqlSrc.indexOf('export const publishedRoleSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    for (const h of [
      'activeRecipientsSql', 'idNameByUserIdsSql', 'verifiedUserEmailSql', 'idNameEmailByUserIdsSql',
      'roleAllowsSql', 'samePersonSql', 'samePersonQuerySql', 'assignmentActiveQuerySql',
      'memberForShareSql', 'protectedMigrationIdentitySql',
    ]) expect((sqlSrc.match(new RegExp(`export const ${h} =`, 'g')) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r16 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r16 BEFORE=534 baseline', () => {
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
    expect(total).toBeLessThan(534);
  });
});
