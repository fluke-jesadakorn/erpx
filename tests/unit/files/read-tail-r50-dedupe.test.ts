import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { DISTINCT_SET_ROW_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r50-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const BASE = '4ed7a6ff2f43c976ebbeb9803deec80ddb36bdbf';
const HELPER = 'adapters/read-tail-r50-source-sql.ts';
const LEAVE = 'adapters/leave-evidence-sources.ts';
const SR = 'adapters/attachments/service-repository.ts';
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

/** Verbatim byte oracle. The head ends mid-identifier: the canonical witness
 * completes it with `id,...` and the generic set-row witness with
 * `company_id,...`; both keep their own FROM/WHERE/ORDER BY tail and binds. */
const HEAD = 'SELECT DISTINCT set_row.';
const HEAD_SHA = 'e34ef9843e0667106b7a9500ecf72e08ba0173b1449bb1b72ae61ef936fa04ff';

/** Render each of the two sites back to its exact pre-r50 inline literal. */
const stripImport = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r50-source-sql') && !line.includes('read-tail-r51-source-sql'))
  .join('\n');
const render = (text: string) => stripImport(text)
  .replaceAll('${DISTINCT_SET_ROW_HEAD_SQL}', HEAD)
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('closed')}", "SELECT 1 FROM attachment_links closed WHERE closed.attachment_id=a.id AND closed.state IN (")
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('l')}", "SELECT 1 FROM attachment_links l WHERE l.attachment_id=a.id AND l.state IN (");

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

describe('Files read-tail r50 byte-oracle dedupe', () => {
  test('the head is an exact read-only byte oracle with a mid-identifier split', () => {
    expect(DISTINCT_SET_ROW_HEAD_SQL).toBe(HEAD);
    expect(HEAD.length).toBe(24);
    expect(sha(HEAD)).toBe(HEAD_SHA);
    expect(count(HEAD, 'SELECT ')).toBe(1);
    expect(HEAD).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(HEAD.endsWith('set_row.')).toBe(true);
    expect('SELECT DISTINCT set_row.id,source.id source_id'.startsWith(HEAD)).toBe(true);
    expect('SELECT DISTINCT set_row.company_id,set_row.target_id'.startsWith(HEAD)).toBe(true);
    expect('id,source.id source_id').not.toBe('company_id,set_row.target_id');
    const helper = src(HELPER);
    expect(count(helper, 'export const DISTINCT_SET_ROW_HEAD_SQL =')).toBe(1);
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
  });

  test('no residual inline literal: the head lives only in its helper', () => {
    let residual = 0;
    for (const path of tsFiles(srcRoot)) residual += count(readFileSync(path, 'utf8'), HEAD);
    expect(residual).toBe(1); // the helper's own declaration
    for (const rel of [LEAVE, SR]) {
      const text = src(rel);
      expect(count(text, HEAD)).toBe(0);
      expect(count(text, '${DISTINCT_SET_ROW_HEAD_SQL}')).toBe(1);
    }
  });

  test('rendered SQL is byte-identical to BASE; binds, locks and witness recheck unchanged', () => {
    for (const rel of [LEAVE, SR]) {
      const base = before(rel);
      const after = src(rel);
      expect(render(after)).toBe(base);
      for (const needle of [
        '.bind(', 'nativeWrite(', 'FOR UPDATE', 'FOR SHARE', 'FOR KEY SHARE',
        'LOCK TABLE', 'pg_advisory_xact_lock', 'ORDER BY',
      ]) {
        expect(count(after, needle)).toBe(count(base, needle));
      }
    }
    const sr = src(SR);
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('db.prepare(advisoryLockSql)');
    expect(sr).toContain('${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL} AND link.state=\'draft\'');
    expect(sr).toContain('${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL}) AND set_row.created_at<=?)');
  });

  test('each site composes the head and keeps its own divergent tail and binds', () => {
    const leave = src(LEAVE);
    expect(leave).toContain('${DISTINCT_SET_ROW_HEAD_SQL}id,source.id source_id');
    expect(leave).toContain('.bind(leaveRequestId, purposeKey)');
    expect(leave).toContain('ORDER BY set_row.id,source.id');

    const sr = src(SR);
    expect(sr).toContain('${DISTINCT_SET_ROW_HEAD_SQL}company_id,set_row.target_id');
    expect(sr).toContain('.bind(facts.now, facts.arg2)');
    expect(sr).toContain('ORDER BY set_row.company_id,set_row.target_id');
  });

  test('retirement:ts-statements is 225, strictly below the r49 BEFORE=226 (226->225)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // The two inline DISTINCT set-row sites fold into one helper (-1 total).
    expect(literal).toBe(223);
    expect(total).toBe(224);
    expect(literal).toBeLessThan(226);
    expect(total).toBeLessThan(227);
  });
});
