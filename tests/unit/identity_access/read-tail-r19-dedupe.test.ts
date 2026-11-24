import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { revokedSessionProbeSql } from '../../../packages/server-identity_access/src/adapters/iam-probe-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sqlSrc = read('adapters/iam-probe-sql.ts');
const legalSrc = read('adapters/legal-ai-actor-facts.ts');
const recordSrc = read('adapters/record-source-authority.ts');
const txSrc = read('adapters/iam-transactions.ts');

const qmarkProbe = 'SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id=? AND user_id=?';
const pgProbe = 'SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1 AND company_id=$2 AND user_id=$3';

describe('Identity access read-tail r19 revoked-session probe family', () => {
  test('renders byte-identical SQL to every pre-dedupe caller literal', () => {
    expect(revokedSessionProbeSql('?', '?', '?')).toBe(qmarkProbe);
    expect(`AND NOT EXISTS(${revokedSessionProbeSql('?', '?', '?')})`).toBe(`AND NOT EXISTS(${qmarkProbe})`);
    expect(revokedSessionProbeSql('$1', '$2', '$3')).toBe(pgProbe);
  });

  test('is one owner helper with two qmark callers and one pg caller, no inline literal left behind', () => {
    expect((sqlSrc.match(/export const revokedSessionProbeSql =/g) ?? []).length).toBe(1);
    expect((legalSrc.match(/\brevokedSessionProbeSql\(/g) ?? []).length).toBe(1);
    expect((recordSrc.match(/\brevokedSessionProbeSql\(/g) ?? []).length).toBe(1);
    expect((txSrc.match(/\brevokedSessionProbeSql\(/g) ?? []).length).toBe(1);
    for (const inline of [qmarkProbe, pgProbe]) {
      expect(legalSrc).not.toContain(inline);
      expect(recordSrc).not.toContain(inline);
      expect(txSrc).not.toContain(inline);
    }
  });

  test('introduces no write or lock and leaves locks and r13-r18 helpers untouched', () => {
    const family = sqlSrc.slice(sqlSrc.indexOf('export const revokedSessionProbeSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // Lock and control-plane read statements stay inline; no lock changed.
    expect(read('adapters/account-preferences.ts')).toContain('SELECT id FROM "user" WHERE id=? FOR UPDATE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('FOR UPDATE');
    expect(txSrc).toContain('FOR SHARE');
    expect(read('adapters/iam-migrations.ts')).toContain('FOR SHARE');
    // Every earlier-round helper still has exactly one owner declaration.
    for (const h of [
      'roleAllowsSql', 'samePersonSql', 'samePersonQuerySql', 'assignmentActiveQuerySql',
      'memberForShareSql', 'protectedMigrationIdentitySql', 'activeRecipientsSql',
      'idNameByUserIdsSql', 'verifiedUserEmailSql', 'idNameEmailByUserIdsSql', 'publishedRoleSql',
      'grantProbeSql',
    ]) expect((sqlSrc.match(new RegExp(`export const ${h} =`, 'g')) ?? []).length).toBe(1);
    expect((read('adapters/identity-user-projection-family-r18.ts').match(/export const userAttributeByIdSql =/g) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r19 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r19 BEFORE=530 baseline (r21 lowered 528 to 522)', () => {
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
    expect(total).toBeLessThan(530);
    expect(total).toBeLessThanOrEqual(522);
  });
});
