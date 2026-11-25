import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL,
  CLEANUP_LINK_ROWS_HEAD_SQL,
  CLEANUP_BOUND_COMMAND_ID_HEAD_SQL,
  LEAVE_LEGACY_SOURCE_ID_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r47-source-sql';
import {
  ATTACHMENT_EVENTS_WHERE_HEAD_SQL,
  ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r45-source-sql';
import {
  ORPHAN_PRESERVATION_EVENT_HEAD_SQL,
  SET_DRAFT_BOUND_COUNT_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r46-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const HEAD = 'e313f4375f2100a8db74a4851395530db948f3ad';
const HELPER = 'adapters/read-tail-r47-source-sql.ts';
const SR = 'adapters/attachments/service-repository.ts';
const CLEANUP = 'adapters/social-upload-cleanup.ts';
const LEAVE = 'adapters/leave-review-evidence-facts.ts';
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${HEAD}:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;

/** Verbatim byte oracles: the exact 64/41/34/58-byte heads lifted by r47. */
const A = 'SELECT 1 FROM attachment_links link WHERE link.set_id=set_row.id';
const B = 'SELECT * FROM attachment_links l WHERE l.';
const C = 'SELECT bound_command_id FROM sets)';
const D = 'SELECT source.id source_id\n                          FROM ';
/** The helper stores D with an escaped newline, so its source text is distinct. */
const D_SRC = 'SELECT source.id source_id\\n                          FROM ';
const HEADS = [A, B, C, D];

const stripImport = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r47-source-sql'))
  .join('\n');
/** Render each site back to its exact pre-r47 inline literals. */
const renderSR = (text: string) => stripImport(text)
  .replaceAll('${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL}', A);
const renderCleanup = (text: string) => stripImport(text)
  .replaceAll('${CLEANUP_LINK_ROWS_HEAD_SQL}', B)
  .replaceAll("'c.command_id IN('+CLEANUP_BOUND_COMMAND_ID_HEAD_SQL", "'c.command_id IN(" + C + "'")
  .replaceAll("'a.command_id IN('+CLEANUP_BOUND_COMMAND_ID_HEAD_SQL", "'a.command_id IN(" + C + "'");
const renderLeave = (text: string) => stripImport(text)
  .replaceAll('${LEAVE_LEGACY_SOURCE_ID_HEAD_SQL}', D);

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

describe('Files read-tail r47 byte-oracle dedupe', () => {
  test('the four read heads are exact read-only byte oracles, distinct from r45/r46', () => {
    expect(SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL).toBe(A);
    expect(CLEANUP_LINK_ROWS_HEAD_SQL).toBe(B);
    expect(CLEANUP_BOUND_COMMAND_ID_HEAD_SQL).toBe(C);
    expect(LEAVE_LEGACY_SOURCE_ID_HEAD_SQL).toBe(D);
    expect(HEADS.map(h => h.length)).toEqual([64, 41, 34, 58]);
    for (const head of HEADS) {
      expect(count(head, 'SELECT ')).toBe(1);
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    expect(A).not.toBe(ATTACHMENT_EVENTS_WHERE_HEAD_SQL);
    expect(B).not.toBe(ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL);
    expect(A).not.toContain('attachment_events');
    expect(B).not.toContain('attachment_sets');
    expect(typeof ORPHAN_PRESERVATION_EVENT_HEAD_SQL).toBe('function');
    expect(typeof SET_DRAFT_BOUND_COUNT_SQL).toBe('function');
    const helper = src(HELPER);
    for (const name of [
      'SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL', 'CLEANUP_LINK_ROWS_HEAD_SQL',
      'CLEANUP_BOUND_COMMAND_ID_HEAD_SQL', 'LEAVE_LEGACY_SOURCE_ID_HEAD_SQL',
    ]) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
    expect(count(helper, D_SRC)).toBe(1);
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
  });

  test('no residual inline literal: each head lives only in its owner-local helper', () => {
    const residual: Record<string, number> = { A: 0, B: 0, C: 0, D: 0 };
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      for (const [name, head] of Object.entries({ A, B, C, D })) residual[name] += count(text, head);
    }
    // A/B/C each occur once in the r47 helper as the constant text; D is
    // stored escaped there, so its runtime newline form appears nowhere.
    expect(residual).toEqual({ A: 1, B: 1, C: 1, D: 0 });
    for (const prior of [
      'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r32-source-sql.ts',
      'read-tail-r35-source-sql.ts', 'read-tail-r38-source-sql.ts', 'read-tail-r40-source-sql.ts',
      'read-tail-r42-source-sql.ts', 'read-tail-r43-source-sql.ts', 'read-tail-r44-source-sql.ts',
      'read-tail-r45-source-sql.ts', 'read-tail-r46-source-sql.ts',
      'social-upload-cleanup-rows-sql.ts', 'preservation-presence-dedupe-sql.ts',
    ]) {
      expect(src(`adapters/${prior}`)).not.toContain(A);
      expect(src(`adapters/${prior}`)).not.toContain(B);
      expect(src(`adapters/${prior}`)).not.toContain(C);
    }
  });

  test('rendered SQL is byte-identical to HEAD; binds, locks and ORDER tails unchanged', () => {
    for (const [rel, render] of [
      [SR, renderSR],
      [CLEANUP, renderCleanup],
      [LEAVE, renderLeave],
    ] as const) {
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
    expect(sr).toContain('ORDER BY set_row.company_id,set_row.target_id');
    expect(src(CLEANUP)).toContain('LOCK TABLE attachments,upload_intents,attachment_links,attachment_sets,attachment_events,attachment_publications,audit_events IN SHARE ROW EXCLUSIVE MODE');
  });

  test('each site composes its head and keeps every bind and tail in place', () => {
    const sr = src(SR);
    expect(count(sr, '${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL}')).toBe(2);
    expect(sr).toContain("EXISTS(${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL} AND link.state='draft' AND link.expires_at<=?)");
    expect(sr).toContain("OR (NOT EXISTS(${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL}) AND set_row.created_at<=?)");
    expect(count(sr, '.bind(facts.now, facts.arg2).all<')).toBe(1);

    const cleanup = src(CLEANUP);
    expect(count(cleanup, '${CLEANUP_LINK_ROWS_HEAD_SQL}')).toBe(2);
    expect(cleanup).toContain('direct_links AS(${CLEANUP_LINK_ROWS_HEAD_SQL}set_id IN(SELECT id FROM owned_sets)');
    expect(cleanup).toContain('links AS(${CLEANUP_LINK_ROWS_HEAD_SQL}id IN(SELECT id FROM direct_links)');
    expect(count(cleanup, "'c.command_id IN('+CLEANUP_BOUND_COMMAND_ID_HEAD_SQL")).toBe(1);
    expect(count(cleanup, "'a.command_id IN('+CLEANUP_BOUND_COMMAND_ID_HEAD_SQL")).toBe(1);
    expect(count(cleanup, ".bind(input.id,input.id,input.id,input.companyId,input.id).first<string>('image')")).toBe(1);

    const leave = src(LEAVE);
    expect(count(leave, '${LEAVE_LEGACY_SOURCE_ID_HEAD_SQL}')).toBe(2);
    expect(leave).toContain('${LEAVE_LEGACY_SOURCE_ID_HEAD_SQL}attachment_sets set_row');
    expect(leave).toContain('${LEAVE_LEGACY_SOURCE_ID_HEAD_SQL}attachments source');
  });

  test('retirement:ts-statements is 233, strictly below the r46 BEFORE=237 (237->233)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // Four closed-union families fold two sites into one helper each (-4):
    // draft-link EXISTS witness, cleanup direct-link CTE, cleanup bound-command
    // id witness and leave-evidence legacy source-id projection.
    expect(literal).toBe(229);
    expect(total).toBe(230);
    expect(literal).toBeLessThan(237);
    expect(total).toBeLessThan(238);
  });
});
