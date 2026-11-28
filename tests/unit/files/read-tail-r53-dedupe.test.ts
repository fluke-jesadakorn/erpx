import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { SOURCE_STATE_HEAD_SQL } from '../../../modules/files/server/src/adapters/read-tail-r53-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'modules/files/server/src');
// Fixed pre-r53 baseline: both inline source-state heads are still literal in
// this commit, so the rendered-SQL oracles stay valid after the slice is
// integrated and HEAD advances.
const BASE = '6d08598b3c2af7d5bbebc8cbce2503ac80c91bfb';
const HELPER = 'adapters/read-tail-r53-source-sql.ts';
const SR = 'adapters/attachments/service-repository.ts';

const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:modules/files/server/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

/** The single retired inline head and the two divergent tails. */
const HEAD = 'SELECT source_state';
type Site = {
  alias: 'plain' | 'legal';
  tail: string;
  sql: string;
  sqlSha: string;
  bytes: number;
  binds: number;
};
const SITES: Site[] = [
  {
    alias: 'plain',
    tail: ' FROM attachments WHERE id=?',
    sql: 'SELECT source_state FROM attachments WHERE id=?',
    sqlSha: '2eac950b596b6ef636614913a7c3dfccf0f15b6f6278d8e04113e410e5d89a12',
    bytes: 47,
    binds: 1,
  },
  {
    alias: 'legal',
    tail: ',legal_hold FROM attachments WHERE id=?',
    sql: 'SELECT source_state,legal_hold FROM attachments WHERE id=?',
    sqlSha: '9fbf387acc26f4a17d1412ac2e171bbb03f4a4d330a4dd946b5290aeb91ed599',
    bytes: 58,
    binds: 1,
  },
];

const lockRe = /INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const renderSql = (site: Site) => SOURCE_STATE_HEAD_SQL + site.tail;
/** Render the owner file back to BASE: substitute the composition form, drop the import. */
const render = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r53-source-sql'))
  .join('\n')
  .replaceAll('SOURCE_STATE_HEAD_SQL + "', '"' + HEAD);

describe('Files read-tail r53 per-site byte oracles', () => {
  test('the shared head is a 19-byte read-only source-state projection', () => {
    expect(SOURCE_STATE_HEAD_SQL).toBe(HEAD);
    expect(sha(HEAD)).toBe('192eae6b027b37d4af0d94d61f87b92417780aa3f14ef2e9481834b8f2a7f5e4');
    expect(Buffer.byteLength(HEAD)).toBe(19);
    expect(count(HEAD, 'SELECT ')).toBe(1);
    expect(HEAD.startsWith('SELECT ')).toBe(true);
    expect(HEAD.endsWith('source_state')).toBe(true);
    expect(HEAD).not.toMatch(lockRe);
  });

  for (const site of SITES) {
    test(`${site.alias} source-state witness: head and tail render byte-exact`, () => {
      expect(renderSql(site)).toBe(site.sql);
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(Buffer.byteLength(site.sql)).toBe(site.bytes);
      expect(count(site.sql, 'SELECT ')).toBe(1);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
      expect(site.sql).not.toMatch(lockRe);
      expect(site.sql.startsWith(HEAD)).toBe(true);
      expect(site.tail.startsWith(HEAD)).toBe(false);
      expect(site.tail.length).toBe(site.bytes - HEAD.length);
    });
  }

  test('the two tails were one byte-identical head with divergent tails', () => {
    expect(SITES[0].tail).not.toBe(SITES[1].tail);
    expect(SITES[0].tail).toContain(' FROM attachments WHERE id=?');
    expect(SITES[1].tail).toContain(',legal_hold FROM attachments WHERE id=?');
    expect(SITES[0].bytes).not.toBe(SITES[1].bytes);
  });
});

describe('Files read-tail r53 source-state head fold', () => {
  test('one owner-local renderer replaces the two inline heads', () => {
    const helper = src(HELPER);
    expect(count(helper, 'export const SOURCE_STATE_HEAD_SQL =')).toBe(1);
    expect(helper).toContain('projection-head-sources.json');
    expect(count(helper, 'SELECT ')).toBe(0);
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
    expect(helper.split('\n').find(l => l.startsWith('export const'))!).not.toMatch(lockRe);

    // No statement token remains in TypeScript; the head lives in the native contract.
    let residual = 0;
    for (const path of tsFiles(srcRoot)) residual += count(readFileSync(path, 'utf8'), HEAD);
    expect(residual).toBe(0);
  });

  test('each site keeps its own tail and bind', () => {
    const sr = src(SR);
    expect(count(sr, 'SOURCE_STATE_HEAD_SQL + " FROM attachments WHERE id=?"')).toBe(1);
    expect(count(sr, 'SOURCE_STATE_HEAD_SQL + ",legal_hold FROM attachments WHERE id=?"')).toBe(1);
    expect(count(sr, 'db.prepare(SOURCE_STATE_HEAD_SQL + "')).toBe(2);
    expect(count(sr, 'db.prepare(SOURCE_STATE_HEAD_SQL + " FROM attachments WHERE id=?").bind(facts.attachmentId)')).toBe(1);
    expect(count(sr, 'db.prepare(SOURCE_STATE_HEAD_SQL + ",legal_hold FROM attachments WHERE id=?").bind(facts.attachmentId)')).toBe(1);
  });

  test('rendered SQL is byte-identical to BASE; binds, locks, native and ORDER BY unchanged', () => {
    const base = before(SR);
    const after = src(SR);
    expect(render(after)).toBe(base);
    for (const needle of [
      '.bind(', 'nativeWrite(', 'FOR UPDATE', 'FOR SHARE', 'FOR KEY SHARE',
      'LOCK TABLE', 'pg_advisory_xact_lock', 'ORDER BY',
    ]) {
      expect(count(after, needle)).toBe(count(base, needle));
    }

    // Excluded advisory/row-lock statements and native registry writes stay untouched.
    const sr = src(SR);
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('db.prepare(advisoryLockSql)');
  });
});

describe('Files read-tail r53 KPI pins', () => {
  test('retirement:ts-statements is 190, strictly below the r52 BEFORE=223 (223->222)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // Two inline heads -> one alias-free owner-local renderer (-1).
    expect(literal).toBe(190);
    expect(total).toBe(191);
    expect(literal).toBeLessThan(223);
    expect(total).toBeLessThan(224);
  });
});
