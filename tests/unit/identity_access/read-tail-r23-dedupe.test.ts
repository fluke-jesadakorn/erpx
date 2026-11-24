import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { projectAuthorityProbeSql } from '../../../packages/server-identity_access/src/adapters/iam-probe-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const helperSrc = read('adapters/iam-probe-sql.ts');
const projectSrc = read('adapters/project-structure-sources.ts');
const head = 'SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)';
const projectAuthority = `${head} AND iam_role_allows(role,company_id,'records.read')`;
const workAuthority = head;

describe('Identity access read-tail r23 project authority probe family', () => {
  test('renders byte-identical SQL to both pre-dedupe literals', () => {
    expect(projectAuthorityProbeSql(" AND iam_role_allows(role,company_id,'records.read')")).toBe(projectAuthority);
    expect(projectAuthorityProbeSql('')).toBe(workAuthority);
  });

  test('is one owner helper with two call sites and no inline literal left behind', () => {
    expect((helperSrc.match(/export const projectAuthorityProbeSql =/g) ?? []).length).toBe(1);
    expect((projectSrc.match(/\bprojectAuthorityProbeSql\b/g) ?? []).length).toBe(3); // import + two call sites
    expect(projectSrc).not.toContain(head);
    expect(projectSrc).toContain('iam_role_allows(role,company_id,\'records.read\')');
  });

  test('introduces no write or lock and leaves r2-r22 helpers untouched', () => {
    const newHelper = helperSrc.slice(helperSrc.indexOf('export const projectAuthorityProbeSql ='));
    expect(newHelper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(read('adapters/account-preferences.ts')).toContain('SELECT id FROM "user" WHERE id=? FOR UPDATE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('FOR UPDATE');
    for (const h of [
      'roleAllowsSql', 'samePersonSql', 'samePersonColumnSql', 'samePersonQuerySql',
      'assignmentActiveQuerySql', 'memberForShareSql', 'protectedMigrationIdentitySql',
      'activeRecipientsSql', 'idNameByUserIdsSql', 'verifiedUserEmailSql',
      'idNameEmailByUserIdsSql', 'publishedRoleSql', 'grantProbeSql',
      'revokedSessionProbeSql', 'sessionByUserSql', 'sessionExistsSql',
    ]) expect((helperSrc.match(new RegExp(`export const ${h} =`, 'g')) ?? []).length, h).toBe(1);
    expect((read('adapters/identity-user-projection-family-r18.ts').match(/export const userAttributeByIdSql =/g) ?? []).length).toBe(1);
    expect((read('adapters/identity-user-projection-family-r21.ts').match(/export const userAttributeByIdExtendedSql =/g) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r23 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r23 BEFORE=521 baseline (expected 520)', () => {
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
    expect(total).toBeLessThan(521);
    expect(total).toBeLessThanOrEqual(520);
  });
});
