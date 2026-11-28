import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { userAttributeByIdExtendedSql } from '../../../modules/identity-access/server/src/adapters/identity-user-projection-family-r21';

const srcRoot = resolve(__dirname, '../../../modules/identity-access/server/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const family21Src = read('adapters/identity-user-projection-family-r21.ts');
const family18Src = read('adapters/identity-user-projection-family-r18.ts');
const serviceSrc = read('adapters/iam-service.ts');
const authSrc = read('adapters/iam-auth.ts');
const recoverySrc = read('adapters/account-recovery.ts');
const startupSrc = read('adapters/project-worker-startup-subject.ts');

describe('Identity access read-tail r21 extended single-id user attribute family', () => {
  test('renders byte-identical SQL to every pre-dedupe wrapper literal', () => {
    expect(userAttributeByIdExtendedSql('id')).toBe('SELECT id FROM "user" WHERE id=?');
    expect(userAttributeByIdExtendedSql('name,email')).toBe('SELECT name,email FROM "user" WHERE id=?');
    expect(userAttributeByIdExtendedSql('email')).toBe('SELECT email FROM "user" WHERE id=?');
    expect(userAttributeByIdExtendedSql('id,name,email,"updatedAt"')).toBe('SELECT id,name,email,"updatedAt" FROM "user" WHERE id=?');
  });

  test('is one extended owner helper with four callers and no inline literal left behind', () => {
    expect((family21Src.match(/export const userAttributeByIdExtendedSql =/g) ?? []).length).toBe(1);
    expect((serviceSrc.match(/\buserAttributeByIdExtendedSql\(/g) ?? []).length).toBe(1);
    expect((authSrc.match(/\buserAttributeByIdExtendedSql\(/g) ?? []).length).toBe(1);
    expect((recoverySrc.match(/\buserAttributeByIdExtendedSql\(/g) ?? []).length).toBe(1);
    expect((startupSrc.match(/\buserAttributeByIdExtendedSql\(/g) ?? []).length).toBe(1);
    for (const inline of [
      'SELECT id FROM "user" WHERE id=?',
      'SELECT name,email FROM "user" WHERE id=?',
      'SELECT email FROM "user" WHERE id=?',
      'SELECT id,name,email,"updatedAt" FROM "user" WHERE id=?',
    ]) {
      expect(serviceSrc).not.toContain(inline);
      expect(authSrc).not.toContain(inline);
      expect(recoverySrc).not.toContain(inline);
      expect(startupSrc).not.toContain(inline);
    }
  });

  test('introduces no write or lock and leaves r18 closed union, locks and r2-r20 helpers untouched', () => {
    expect(family21Src).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // r18's closed union and its helper declaration stay untouched.
    expect((family18Src.match(/export const userAttributeByIdSql =/g) ?? []).length).toBe(1);
    expect(family18Src).toContain("export type UserAttributeProjection = 'id,email' | 'id,name' | 'id,name,email';");
    // The duplicate FOR UPDATE / FOR SHARE lock families stay inline; no lock changed.
    expect(read('adapters/account-preferences.ts')).toContain('SELECT id FROM "user" WHERE id=? FOR UPDATE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('FOR UPDATE');
    // Every earlier-round helper still has exactly one owner declaration.
    for (const h of [
      'roleAllowsSql', 'samePersonSql', 'samePersonQuerySql', 'assignmentActiveQuerySql',
      'memberForShareSql', 'protectedMigrationIdentitySql', 'activeRecipientsSql',
      'idNameByUserIdsSql', 'verifiedUserEmailSql', 'idNameEmailByUserIdsSql', 'publishedRoleSql',
      'grantProbeSql', 'revokedSessionProbeSql',
    ]) expect((read('adapters/iam-probe-sql.ts').match(new RegExp(`export const ${h} =`, 'g')) ?? []).length, h).toBe(1);
  });
});

describe('Identity access read-tail r21 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r21 BEFORE=525 baseline (expected 522)', () => {
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
    expect(total).toBeLessThan(525);
    expect(total).toBeLessThanOrEqual(522);
  });
});
