import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { userAttributeByIdSql } from '../../../packages/server-identity_access/src/adapters/identity-user-projection-family-r18';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const familySrc = read('adapters/identity-user-projection-family-r18.ts');
const dirSrc = read('adapters/people-directory-facts.ts');
const profileSrc = read('adapters/profile-account.ts');
const sqlSrc = read('adapters/iam-probe-sql.ts');

describe('Identity access read-tail r18 single-id user attribute family', () => {
  test('renders byte-identical SQL to every pre-dedupe wrapper literal', () => {
    expect(userAttributeByIdSql('id,email')).toBe('SELECT id,email FROM "user" WHERE id=?');
    expect(userAttributeByIdSql('id,name')).toBe('SELECT id,name FROM "user" WHERE id=?');
    expect(userAttributeByIdSql('id,name,email')).toBe('SELECT id,name,email FROM "user" WHERE id=?');
  });

  test('is one owner helper with three callers and no inline literal left behind', () => {
    expect((familySrc.match(/export const userAttributeByIdSql =/g) ?? []).length).toBe(1);
    expect((dirSrc.match(/\buserAttributeByIdSql\(/g) ?? []).length).toBe(2);
    expect((profileSrc.match(/\buserAttributeByIdSql\(/g) ?? []).length).toBe(1);
    for (const inline of [
      'SELECT id,email FROM "user" WHERE id=?',
      'SELECT id,name FROM "user" WHERE id=?',
      'SELECT id,name,email FROM "user" WHERE id=?',
    ]) {
      expect(dirSrc).not.toContain(inline);
      expect(profileSrc).not.toContain(inline);
    }
  });

  test('introduces no write or lock and leaves locks and r13-r17 helpers untouched', () => {
    expect(familySrc).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // The duplicate FOR UPDATE / FOR SHARE lock families stay inline; no lock changed.
    expect(read('adapters/account-preferences.ts')).toContain('SELECT id FROM "user" WHERE id=? FOR UPDATE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('FOR UPDATE');
    // Every earlier-round helper still has exactly one owner declaration.
    for (const h of [
      'roleAllowsSql', 'samePersonSql', 'samePersonQuerySql', 'assignmentActiveQuerySql',
      'memberForShareSql', 'protectedMigrationIdentitySql', 'activeRecipientsSql',
      'idNameByUserIdsSql', 'verifiedUserEmailSql', 'idNameEmailByUserIdsSql', 'publishedRoleSql',
      'grantProbeSql',
    ]) expect((sqlSrc.match(new RegExp(`export const ${h} =`, 'g')) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r18 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r18 BEFORE=532 baseline', () => {
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
    expect(total).toBeLessThan(532);
  });
});
