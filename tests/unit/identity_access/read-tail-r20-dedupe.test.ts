import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  samePersonColumnSql,
  samePersonQuerySql,
  sessionByUserSql,
  sessionExistsSql,
} from '../../../modules/identity-access/server/src/adapters/iam-probe-sql';

const srcRoot = resolve(__dirname, '../../../modules/identity-access/server/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const sqlSrc = read('adapters/iam-probe-sql.ts');
const apiSrc = read('adapters/iam-api.ts');
const readsSrc = read('adapters/iam-service-reads.ts');
const peopleSrc = read('adapters/people-evaluation-facts.ts');
const pushSrc = read('adapters/collaboration-push-session.ts');

// Pre-dedupe rendered literals; whole-string byte identity is the oracle, so no
// quote, predicate or token may drift from the original call sites.
const oracle = {
  samePg: { sql: 'SELECT iam_same_person($1,$2,$3) same', sha: '7a43586e956bd2c8945dd0d328ee77058c104ad7b905e4e5ba1f2f65ed81e822' },
  sameQmark: { sql: 'SELECT iam_same_person(?,?,?) same', sha: '89d25802d187f38145e4d5f333b8c38a937a9f8db32e3236028dcb52bfb2361e' },
  sessionsApi: { sql: 'SELECT id,"createdAt","expiresAt","userAgent" FROM session WHERE "userId"=?', sha: 'a1fe46c2ead75638bd0d8b983805432e13a58a7e605b662a49983c38887f386a' },
  sessionsService: { sql: 'SELECT id,"createdAt","expiresAt","userAgent" FROM session WHERE "userId"=? ORDER BY "createdAt" DESC', sha: '669a658f3503d8a1eccef8063ba01d0686f4c37db35f06605683f71a430e26d1' },
  existsApi: { sql: 'SELECT 1 FROM session WHERE id=? AND "userId"=?', sha: '2da4a779305de0f3cdf1c478b0b18b5eced4fcd0631c43868c4709a6b899a6da' },
  existsPush: { sql: 'SELECT 1 FROM session WHERE id=? AND "userId"=? AND "expiresAt">?', sha: '5606aeeb75f5e13b46a4a9d79dfb143d63bee0fd1a5547b4bede0814b4d27637' },
} as const;

describe('Identity access read-tail r20 shared read helpers', () => {
  test('render byte-identically to every pre-dedupe caller literal and stay read-only', () => {
    const members: Array<[string, string, (typeof oracle)[keyof typeof oracle]]> = [
      ['same pg (r19 export)', samePersonQuerySql, oracle.samePg],
      ['same pg', samePersonColumnSql('$1', '$2', '$3'), oracle.samePg],
      ['same qmark', samePersonColumnSql('?', '?', '?'), oracle.sameQmark],
      ['sessions api', sessionByUserSql(''), oracle.sessionsApi],
      ['sessions service', sessionByUserSql(' ORDER BY "createdAt" DESC'), oracle.sessionsService],
      ['exists api', sessionExistsSql(''), oracle.existsApi],
      ['exists push', sessionExistsSql(' AND "expiresAt">?'), oracle.existsPush],
    ];
    for (const [name, actual, member] of members) {
      expect(actual, name).toBe(member.sql);
      expect(sha(actual), name).toBe(member.sha);
      readOnly(actual);
    }
  });

  test('are one owner declaration each with no inline literal left behind', () => {
    for (const helper of ['samePersonColumnSql', 'sessionByUserSql', 'sessionExistsSql']) {
      expect((sqlSrc.match(new RegExp(`export const ${helper} =`, 'g')) ?? []).length, helper).toBe(1);
    }
    expect((sqlSrc.match(/\bsamePersonColumnSql\(/g) ?? []).length).toBe(1); // the r19 pg export
    // The session-by-user SELECT is retired from both call sites: the IAM
    // service reads port and the account IAM catch-all probe now use the
    // mounted native lane 45 session-list read. The helper survives only as
    // the byte-identity oracle pinned by the Rust TS_ORACLE_SQL contract.
    expect((apiSrc.match(/\bsessionByUserSql\(/g) ?? []).length).toBe(0);
    expect((apiSrc.match(/\bsessionExistsSql\(/g) ?? []).length).toBe(1);
    expect((readsSrc.match(/\bsessionByUserSql\(/g) ?? []).length).toBe(0);
    expect((peopleSrc.match(/\bsamePersonColumnSql\(/g) ?? []).length).toBe(1);
    expect((pushSrc.match(/\bsessionExistsSql\(/g) ?? []).length).toBe(1);
    for (const literal of [oracle.samePg.sql, oracle.sameQmark.sql]) {
      expect(peopleSrc).not.toContain(literal);
    }
    expect(apiSrc).not.toContain(oracle.sessionsApi.sql);
    expect(readsSrc).not.toContain(oracle.sessionsService.sql);
    expect(apiSrc).not.toContain(oracle.existsApi.sql);
    expect(pushSrc).not.toContain(oracle.existsPush.sql);
    const samePersonFamily = sqlSrc.slice(
      sqlSrc.indexOf('export const samePersonColumnSql'),
      sqlSrc.indexOf('export const samePersonQuerySql'),
    );
    const sessionFamily = sqlSrc.slice(sqlSrc.indexOf('export const sessionByUserSql'));
    for (const family of [samePersonFamily, sessionFamily]) {
      expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  test('leaves locks, control-plane reads and r2-r19 helpers untouched', () => {
    // Every earlier-round helper still has exactly one owner declaration.
    for (const h of [
      'roleAllowsSql', 'samePersonSql', 'samePersonQuerySql', 'assignmentActiveQuerySql',
      'memberForShareSql', 'protectedMigrationIdentitySql', 'activeRecipientsSql',
      'idNameByUserIdsSql', 'verifiedUserEmailSql', 'idNameEmailByUserIdsSql', 'publishedRoleSql',
      'grantProbeSql', 'revokedSessionProbeSql',
    ]) expect((sqlSrc.match(new RegExp(`export const ${h} =`, 'g')) ?? []).length, h).toBe(1);
    expect(read('adapters/account-preferences.ts')).toContain('SELECT id FROM "user" WHERE id=? FOR UPDATE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('FOR UPDATE');
    expect(read('adapters/iam-transactions.ts')).toContain('FOR SHARE');
    expect(read('adapters/iam-migrations.ts')).toContain('FOR SHARE');
    expect(read('adapters/work-ui-layout-authority.ts')).toContain('FOR SHARE');
    // The session read/adapter files keep their other statements unchanged.
    expect(read('adapters/iam-api.ts')).toContain(`'SELECT id FROM session WHERE "userId"=?'`);
    expect(pushSrc).toContain('FROM active_work_contexts c JOIN assignments a');
  });
});

describe('Identity access read-tail r20 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r20 BEFORE=528 baseline (r21 lowered 525 to 522)', () => {
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
    expect(total).toBeLessThan(528);
    expect(total).toBeLessThanOrEqual(522);
  });
});
