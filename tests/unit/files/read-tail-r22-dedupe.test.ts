import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  LINK_SET_WHERE_EXISTS_SQL,
  PACKAGE_OBLIGATION_SCOPE_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-dedupe-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');

const WHERE_EXISTS = 'EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.';
const SCOPE_PREFIX = 'WITH scope AS(SELECT ?::text company_id),sets AS MATERIALIZED(SELECT ';
const WHERE_NEEDLE = '${LINK_SET_WHERE_EXISTS_SQL}';
const SCOPE_NEEDLE = '${PACKAGE_OBLIGATION_SCOPE_SQL}';

const readOnly = (sql: string) => {
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|\bcount\s*\(/i);
};

const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

describe('Files read-tail r22 parameterized-family dedupe', () => {
  test('canonical helpers are the exact byte-identical SQL fragments', () => {
    expect(LINK_SET_WHERE_EXISTS_SQL).toBe(WHERE_EXISTS);
    expect(PACKAGE_OBLIGATION_SCOPE_SQL).toBe(SCOPE_PREFIX);
    readOnly(LINK_SET_WHERE_EXISTS_SQL);
    readOnly(PACKAGE_OBLIGATION_SCOPE_SQL);
    // Rendering the interpolations reproduces the original inlined bytes exactly.
    expect(`AND NOT ${LINK_SET_WHERE_EXISTS_SQL}attachment_id=a.id`).toBe(`AND NOT ${WHERE_EXISTS}attachment_id=a.id`);
    expect(`${PACKAGE_OBLIGATION_SCOPE_SQL}s.* FROM attachment_sets s,scope`).toBe(`${SCOPE_PREFIX}s.* FROM attachment_sets s,scope`);
  });

  test('helpers are defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-dedupe-sql.ts');
    expect(shared.split('LINK_SET_WHERE_EXISTS_SQL =')).toHaveLength(2);
    expect(shared.split('PACKAGE_OBLIGATION_SCOPE_SQL =')).toHaveLength(2);
  });

  test('link/set WHERE-exists guard is composed, never inlined, in both consumers', () => {
    const sr = src('attachments/service-repository.ts');
    const ff = src('field-file-writer-owner.ts');
    expect(sr).not.toContain(WHERE_EXISTS);
    expect(ff).not.toContain(WHERE_EXISTS);
    expect(sr.match(/\$\{LINK_SET_WHERE_EXISTS_SQL\}/g) ?? []).toHaveLength(1);
    expect(ff.match(/\$\{LINK_SET_WHERE_EXISTS_SQL\}/g) ?? []).toHaveLength(4);
    expect(sr).toContain(`AND NOT ${WHERE_NEEDLE}attachment_id=a.id AND s.retention_policy_key IS NOT NULL`);
    expect(ff).toContain(`${WHERE_NEEDLE}id=? AND s.company_id=?`);
  });

  test('package-obligation scope prefix is composed, never inlined, in all four sources', () => {
    const names = [
      'ecommerce-package-obligation-sources.ts',
      'marketing-package-obligation-sources.ts',
      'performance-package-obligation-sources.ts',
      'portal-package-obligation-sources.ts',
    ];
    for (const name of names) {
      const text = src(name);
      expect(text).not.toContain(SCOPE_PREFIX);
      expect(text.match(/\$\{PACKAGE_OBLIGATION_SCOPE_SQL\}/g) ?? []).toHaveLength(1);
    }
    expect(src('ecommerce-package-obligation-sources.ts')).toContain(`${SCOPE_NEEDLE}s.* FROM attachment_sets s,scope`);
    expect(src('portal-package-obligation-sources.ts')).toContain(`${SCOPE_NEEDLE}native.* FROM attachment_sets native,scope`);
  });

  test('advisory locks and count(*) pairs remain untouched', () => {
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('SELECT count(DISTINCT candidate.id)');
    expect(sr).toContain('SELECT COALESCE(max(revision),0)+1 AS next_revision');
    expect(src('finance-review-file-sources.ts')).toContain("'(SELECT count(*) FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id");
  });

  test('r10-r21 shared helpers remain defined and unmodified', () => {
    const tail = src('read-tail-dedupe-sql.ts');
    for (const name of [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'scansAggSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
      'LINK_ATTACHMENT_SELECT_SQL',
    ]) expect(tail).toContain(`export const ${name} =`);
    expect(src('finance-review-file-sources.ts')).toContain('${LINK_SET_ATTACHMENT_EXISTS_SQL}');
    expect(src('attachments/service-repository.ts')).toContain('LINK_SET_ID_SELECT_SQL');
  });

  test('retirement:ts-statements stays strictly below the r22 BEFORE=314 baseline', () => {
    let total = 0;
    let literal = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          total += selectCount(text);
          literal += literalSelectCount(text);
        }
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(314);
    expect(literal).toBeLessThan(314);
  });
});
