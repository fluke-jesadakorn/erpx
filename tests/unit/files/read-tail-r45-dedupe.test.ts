import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ATTACHMENT_EVENTS_WHERE_HEAD_SQL,
  ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r45-source-sql';
import { socialDraftImageExistsSql } from '../../../packages/server-files/src/adapters/social-draft-image-guard-sql';
import { ATTACHMENT_EVENT_EVENT_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r38-source-sql';
import { SANITIZATION_ATTESTED_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r40-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const SR = 'adapters/attachments/service-repository.ts';
const HOME = 'adapters/collaboration-home-sources.ts';
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `HEAD:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;

/** Verbatim byte oracles: the exact 38/36/40-byte heads lifted by r45. */
const H1 = 'SELECT 1 FROM attachment_events WHERE ';
const H2 = 'SELECT * FROM attachment_sets WHERE ';
const H3 = 'EXISTS(SELECT 1 FROM attachments a JOIN ';

/** The single r45 family-2 site: guardPublishImage's own inline literal. The
 * other three collaboration-home sites already used socialDraftImageExistsSql
 * before r45 and must keep using it. The trailing newline disambiguates from
 * guardUploadedImage, which has the same helper prefix on one line. */
const F2_SITE = '${socialDraftImageExistsSql}upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id\n';

const stripImport = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r45-source-sql'))
  .join('\n');
/** Render the two r45 heads back to their original inline literals. */
const render = (text: string) => stripImport(text)
  .replaceAll('ATTACHMENT_EVENTS_WHERE_HEAD_SQL + "', '"' + H1)
  .replaceAll('ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL + "', '"' + H2)
  .replaceAll('${ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL}', H2)
  .replace(F2_SITE, H3 + 'upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id\n');

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

describe('Files read-tail r45 byte-oracle dedupe', () => {
  test('the two shared heads are exact read-only byte oracles, distinct from r38/r40', () => {
    expect(ATTACHMENT_EVENTS_WHERE_HEAD_SQL).toBe(H1);
    expect(ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL).toBe(H2);
    expect(socialDraftImageExistsSql).toBe(H3);
    expect([H1.length, H2.length, H3.length]).toEqual([38, 36, 40]);
    for (const head of [H1, H2, H3]) {
      expect(count(head, 'SELECT ')).toBe(1);
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // Novel: r38 carries an `event` alias and r40 an `attested` alias; r45 is unaliased.
    expect(ATTACHMENT_EVENT_EVENT_HEAD_SQL).toBe('SELECT 1 FROM attachment_events event WHERE event.');
    expect(SANITIZATION_ATTESTED_HEAD_SQL).toContain("attested WHERE attested.");
    expect(H1).not.toContain('event.');
    expect(H1).not.toContain('attested.');
    expect(H1).not.toBe(ATTACHMENT_EVENT_EVENT_HEAD_SQL);
    expect(H1).not.toBe(SANITIZATION_ATTESTED_HEAD_SQL);
    const helper = src('adapters/read-tail-r45-source-sql.ts');
    for (const name of ['ATTACHMENT_EVENTS_WHERE_HEAD_SQL', 'ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
      expect(count(helper, '${' + name + '}')).toBe(0);
    }
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
  });

  test('no residual inline literal: each head lives only in its helper', () => {
    const residual: Record<string, number> = { H1: 0, H2: 0, H3: 0 };
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      residual.H1 += count(text, H1);
      residual.H2 += count(text, H2);
      residual.H3 += count(text, H3);
    }
    // H2 also occurs once in the out-of-scope social-upload-cleanup.ts statement.
    expect(residual).toEqual({ H1: 1, H2: 2, H3: 1 });
    expect(src('adapters/read-tail-r45-source-sql.ts')).toContain(H1);
    expect(src('adapters/read-tail-r45-source-sql.ts')).toContain(H2);
    expect(src('adapters/social-upload-cleanup.ts')).toContain(H2);
    expect(src('adapters/social-draft-image-guard-sql.ts')).toContain(H3);
    for (const prior of [
      'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r32-source-sql.ts',
      'read-tail-r38-source-sql.ts', 'read-tail-r40-source-sql.ts', 'read-tail-r42-source-sql.ts',
      'read-tail-r44-source-sql.ts', 'social-upload-cleanup-rows-sql.ts',
    ]) {
      expect(src(`adapters/${prior}`)).not.toContain(H1);
    }
  });

  test('rendered SQL is byte-identical to HEAD; binds, locks and native writes unchanged', () => {
    for (const rel of [SR, HOME]) {
      const base = before(rel);
      const after = src(rel);
      expect(render(after)).toBe(base);
      for (const needle of ['.bind(', 'nativeWrite(', 'FOR UPDATE', 'LOCK TABLE', 'pg_advisory_xact_lock']) {
        expect(count(after, needle)).toBe(count(base, needle));
      }
    }
    const sr = src(SR);
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('db.prepare(advisoryLockSql)');
    expect(src(HOME)).toContain("nativeWrite('SF_REVOKE_DRAFT'");
  });

  test('each site composes its head and leaves every tail in place', () => {
    const sr = src(SR);
    expect(sr).toContain('ATTACHMENT_EVENTS_WHERE_HEAD_SQL + "set_id=? AND company_id=? AND action=\'set.created\'');
    expect(sr).toContain('ATTACHMENT_EVENTS_WHERE_HEAD_SQL + "attachment_id=? AND action=\'source.bytes_deleted\'"');
    expect(count(sr, 'ATTACHMENT_EVENTS_WHERE_HEAD_SQL + "')).toBe(2);
    expect(sr).toContain('const setByIdSql = ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL + "id=? AND company_id=?";');
    expect(sr).toContain('${ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL}${filters.join(" AND ")} ORDER BY created_at DESC,id DESC');
    expect(count(sr, '${ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL}')).toBe(1);
    const home = src(HOME);
    expect(count(home, F2_SITE)).toBe(1);
    // The three pre-r25 guard sites are untouched.
    expect(count(home, '${socialDraftImageExistsSql}')).toBe(4);
    expect(home).toContain("import { socialDraftImageExistsSql } from './social-draft-image-guard-sql';");
  });

  test('retirement:ts-statements is 239, strictly below the r44 BEFORE=242 (242->239)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // Three heads fold two sites into one each (-3): unaliased attachment_events,
    // attachment_sets star and the guardPublishImage EXISTS prefix.
    expect(literal).toBe(239);
    expect(total).toBe(240);
    expect(literal).toBeLessThan(242);
    expect(total).toBeLessThan(243);
  });
});
