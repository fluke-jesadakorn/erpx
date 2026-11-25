import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { LINK_STATE_EXISTS_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r51-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
// Fixed pre-r51 baseline: the two inline attachment-link state witnesses are
// still literal in this commit, so the rendered-SQL oracles stay valid after
// the slice is integrated and HEAD advances.
const BASE = '68d61c5f76caeb940f54f730e11b7c2ca0a3d903';
const HELPER = 'adapters/read-tail-r51-source-sql.ts';
const SR = 'adapters/attachments/service-repository.ts';

const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

/** The two retired inline heads. Each is the same template under a closed
 * alias union (`closed` | `l`); the state-list/expiry tail stays at its site. */
type Site = { alias: 'closed' | 'l'; tail: string; sql: string; sqlSha: string; bytes: number; binds: number };
const SITES: Site[] = [
  {
    alias: 'closed',
    tail: "'expired','withdrawn') AND closed.expires_at<=?",
    sql: "SELECT 1 FROM attachment_links closed WHERE closed.attachment_id=a.id AND closed.state IN ('expired','withdrawn') AND closed.expires_at<=?",
    sqlSha: 'af4a7f7b3061e075c32a9e72ecd280c2427f361b6cd8914e0fdcce06f61a1997',
    bytes: 138,
    binds: 1,
  },
  {
    alias: 'l',
    tail: "'draft','bound')",
    sql: "SELECT 1 FROM attachment_links l WHERE l.attachment_id=a.id AND l.state IN ('draft','bound')",
    sqlSha: '0039b1866bf55516c19cfb34e5b1b346bfaf32725a61381ec38da9e5359bfeea',
    bytes: 92,
    binds: 0,
  },
];

const HEAD_CLOSED = 'SELECT 1 FROM attachment_links closed WHERE closed.attachment_id=a.id AND closed.state IN (';
const HEAD_L = 'SELECT 1 FROM attachment_links l WHERE l.attachment_id=a.id AND l.state IN (';
const lockRe = /INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

/** Render a site back to its exact pre-r51 inline literal. */
const renderSql = (site: Site) => LINK_STATE_EXISTS_HEAD_SQL(site.alias) + site.tail;
/** Render the whole file back to BASE: substitute the two head calls, drop the import. */
const render = (text: string) => text
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('closed')}", HEAD_CLOSED)
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('l')}", HEAD_L)
  .split('\n')
  .filter(line => !line.includes('read-tail-r51-source-sql') && !line.includes('read-tail-r53-source-sql'))
  .join('\n')
  .replaceAll('SOURCE_STATE_HEAD_SQL + "', '"SELECT source_state');

describe('Files read-tail r51 per-site byte oracles', () => {
  for (const site of SITES) {
    test(`${site.alias} state witness: head and rendered SQL are byte-exact`, () => {
      const head = LINK_STATE_EXISTS_HEAD_SQL(site.alias);
      expect(head).toBe(site.alias === 'closed' ? HEAD_CLOSED : HEAD_L);
      expect(sha(head)).toBe(site.alias === 'closed'
        ? '13cb1005cdc4ae4cd70d5a3a84f83abaf972b6ccda996c92ae6edf9db5aecf0b'
        : '4c81dc1139a57e40d45f3d078018d3e5a4f5db212660e5cbd2ed7076d64e3ef8');
      expect(Buffer.byteLength(head)).toBe(site.alias === 'closed' ? 91 : 76);
      expect(count(head, 'SELECT ')).toBe(1);
      expect(head.startsWith('SELECT 1 FROM attachment_links ')).toBe(true);
      expect(head.endsWith('.state IN (')).toBe(true);
      expect(head).not.toMatch(lockRe);

      expect(renderSql(site)).toBe(site.sql);
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(Buffer.byteLength(site.sql)).toBe(site.bytes);
      expect(count(site.sql, 'SELECT ')).toBe(1);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
      expect(site.sql).not.toMatch(lockRe);
      expect(site.sql.startsWith(head)).toBe(true);
    });
  }

  test('the two heads were one template under different aliases', () => {
    expect(HEAD_CLOSED.replace(/\bclosed\b/g, 'l')).toBe(HEAD_L);
    expect(Buffer.byteLength(HEAD_CLOSED)).toBe(91);
    expect(Buffer.byteLength(HEAD_L)).toBe(76);
    expect(sha(HEAD_CLOSED)).not.toBe(sha(HEAD_L));
  });
});

describe('Files read-tail r51 state-witness fold', () => {
  test('one alias-parameterized renderer replaces the two inline heads', () => {
    const helper = src(HELPER);
    expect(count(helper, 'export const LINK_STATE_EXISTS_HEAD_SQL =')).toBe(1);
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper.split('\n').find(l => l.startsWith('export const'))!).not.toMatch(lockRe);

    let residualClosed = 0, residualL = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      residualClosed += count(text, HEAD_CLOSED);
      residualL += count(text, HEAD_L);
    }
    expect(residualClosed).toBe(0);
    expect(residualL).toBe(0);

    const sr = src(SR);
    expect(count(sr, "${LINK_STATE_EXISTS_HEAD_SQL('closed')}")).toBe(1);
    expect(count(sr, "${LINK_STATE_EXISTS_HEAD_SQL('l')}")).toBe(1);
  });

  test('each site keeps its own tail, statement prefix and binds', () => {
    const sr = src(SR);
    expect(sr).toContain("AND EXISTS(${LINK_STATE_EXISTS_HEAD_SQL('closed')}'expired','withdrawn') AND closed.expires_at<=?)");
    expect(sr).toContain("AND NOT EXISTS(${LINK_STATE_EXISTS_HEAD_SQL('l')}'draft','bound'))");
    expect(sr).toContain('.bind(facts.now, facts.now)');
  });

  test('rendered readAttachments138 SQL is byte-identical to BASE; locks and binds unchanged', () => {
    const base = before(SR);
    const after = src(SR);
    expect(render(after)).toBe(base);
    for (const needle of [
      '.bind(', 'nativeWrite(', 'FOR UPDATE', 'FOR SHARE', 'FOR KEY SHARE',
      'LOCK TABLE', 'pg_advisory_xact_lock', 'ORDER BY',
    ]) {
      expect(count(after, needle)).toBe(count(base, needle));
    }
  });

  test('excluded advisory and row-lock SQL stays untouched', () => {
    const sr = src(SR);
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('db.prepare(advisoryLockSql)');
    const base = before(SR);
    for (const needle of [
      'const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"',
      'const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"',
      'SELECT pg_advisory_xact_lock(hashtextextended(\'legal-files\',0))',
    ]) {
      expect(count(sr, needle)).toBe(count(base, needle));
    }
  });
});

describe('Files read-tail r51 KPI pins', () => {
  test('retirement:ts-statements is 190, strictly below the r50 BEFORE=225 (225->224)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // Two inline state-witness heads -> one alias-parameterized renderer (-1).
    expect(literal).toBe(190);
    expect(total).toBe(191);
    expect(literal).toBeLessThan(225);
    expect(total).toBeLessThan(226);
  });
});
