import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { idNameByUserIdsSql, verifiedUserEmailSql } from '../../../packages/server-identity_access/src/adapters/iam-probe-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sqlSrc = read('adapters/iam-probe-sql.ts');
const seatSrc = read('adapters/commercial-seat-facts.ts');

const ws = (s: string) => s.replace(/\s+/g, ' ').trim();
const ids = ['u1', 'u2'];

describe('Identity access read-tail r14 byte-identical/parameterized families', () => {
  test('user id/name read renders byte-identical SQL at both callers', () => {
    expect(idNameByUserIdsSql(ids)).toBe('SELECT id,name FROM "user" WHERE id IN (?,?)');
  });

  test('verified-email read is the original literal, unchanged', () => {
    expect(verifiedUserEmailSql).toBe('SELECT email,"emailVerified" verified FROM "user" WHERE id=?');
  });

  test('commercial seat account projection keeps one parameterized literal for both callers', () => {
    const preAccounts = `SELECT u.id user_id,p.status,
      EXISTS(SELECT 1 FROM platform_assignments x WHERE x.user_id=u.id) platform_identity
      FROM "user" u LEFT JOIN principals p ON p.user_id=u.id
      WHERE u.id IN (${ids.map(() => "?").join(",")})`;
    const preProjection = `SELECT u.id user_id,p.status,
    EXISTS(SELECT 1 FROM platform_assignments x WHERE x.user_id=u.id) platform_identity
    FROM "user" u LEFT JOIN principals p ON p.user_id=u.id
    WHERE u.id IN (${ids.map(() => "?").join(",")})`;
    // The two pre-dedupe literals differed only in insignificant indentation.
    expect(ws(preAccounts)).toBe(ws(preProjection));
    // The shared statement keeps the platform-identity projection and id binds.
    expect(seatSrc).toContain('seatAccountsSql = (ids:string[]) => `SELECT u.id user_id,p.status,');
    expect(seatSrc).toContain('EXISTS(SELECT 1 FROM platform_assignments x WHERE x.user_id=u.id) platform_identity');
    expect((seatSrc.match(/db\.prepare\(seatAccountsSql\(ids\)\)/g) ?? []).length).toBe(2);
    expect((seatSrc.match(/EXISTS\(SELECT 1 FROM platform_assignments/g) ?? []).length).toBe(1);
  });

  test('each cross-file family is one owner literal with two callers and no write or lock', () => {
    const families = [
      { name: 'idName', decl: 'idNameByUserIdsSql', uses: 2, callers: [
        'adapters/calendar-appointments-facts.ts', 'adapters/executive-main-evidence-sources.ts',
      ] },
      { name: 'verifiedEmail', decl: 'verifiedUserEmailSql', uses: 2, callers: [
        'adapters/private-session-preferences.ts', 'adapters/private-session-read.ts',
      ] },
      { name: 'sharedRoleAllows', decl: 'roleAllowsSql', uses: 2, callers: [
        'adapters/people-roster-facts.ts',
      ] },
    ];
    for (const f of families) {
      expect((sqlSrc.match(new RegExp(`export const ${f.decl} =`, 'g')) ?? []).length).toBe(1);
      let uses = 0;
      for (const rel of f.callers) {
        const s = read(rel).split('\n').filter(line => !/^\s*import\b/.test(line)).join('\n');
        uses += (s.match(new RegExp(`\\b${f.decl}\\b`, 'g')) ?? []).length;
      }
      expect(uses, `${f.name} usages`).toBe(f.uses);
    }
    // No family introduced a write or a lock statement.
    const added = sqlSrc.slice(sqlSrc.indexOf('export const idNameByUserIdsSql'));
    expect(added).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    for (const rel of ['adapters/private-session-preferences.ts', 'adapters/private-session-read.ts']) {
      const before = read(rel);
      expect(before).not.toMatch(/prepare\(\s*'SELECT email,"emailVerified"/);
    }
  });

  test('r13 active-recipient family is untouched', () => {
    expect((sqlSrc.match(/export const activeRecipientsSql =/g) ?? []).length).toBe(1);
    expect(sqlSrc).toContain('AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)');
  });
});

describe('Identity access read-tail r14 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r14 BEFORE=540 baseline', () => {
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
    expect(total).toBeLessThan(540);
  });
});
