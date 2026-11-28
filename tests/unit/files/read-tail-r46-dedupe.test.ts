import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ORPHAN_PRESERVATION_EVENT_HEAD_SQL,
  SET_DRAFT_BOUND_COUNT_SQL,
} from '../../../modules/files/server/src/adapters/read-tail-r46-source-sql';
import { ORPHAN_PRESERVATION_PENDING_SQL } from '../../../modules/files/server/src/adapters/preservation-presence-dedupe-sql';
import {
  ATTACHMENT_EVENTS_WHERE_HEAD_SQL,
  ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL,
} from '../../../modules/files/server/src/adapters/read-tail-r45-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'modules/files/server/src');
const BASE = '81d5eaf835021b61868d75632322dd6add11d738';
const MIG = 'adapters/attachments/migration.ts';
const SR = 'adapters/attachments/service-repository.ts';
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:modules/files/server/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;

/** Verbatim byte oracles: the exact 111/110/157/148-byte heads lifted by r46. */
const PREFIX = "SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='";
const P1 = PREFIX + "preserved'";
const P2 = PREFIX + "restored'";
const COUNT_TAIL = "FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.set_id=? AND s.company_id=? AND l.state IN ('draft','bound')";
const H1 = 'SELECT count(*) AS count ' + COUNT_TAIL;
const H2 = 'SELECT count(*) ' + COUNT_TAIL;
const HEADS = [P1, P2, H1, H2];
/** r47 folded the draft-link EXISTS witness; rendered back only so this
 * r46 oracle still compares against pre-r47 BASE bytes. */
const R47_A = 'SELECT 1 FROM attachment_links link WHERE link.set_id=set_row.id';
/** r49 folded the upload-intent star head; rendered back only so this r46
 * oracle still compares against pre-r49 BASE bytes. */
const R49_A = 'SELECT * FROM upload_intents WHERE ';

const stripImport = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r46-source-sql') && !line.includes('read-tail-r47-source-sql') && !line.includes('read-tail-r49-source-sql') && !line.includes('read-tail-r50-source-sql') && !line.includes('read-tail-r51-source-sql') && !line.includes('read-tail-r53-source-sql') && !line.includes('read-tail-r54-source-sql'))
  .join('\n')
  .replaceAll('SOURCE_STATE_HEAD_SQL + "', '"SELECT source_state');
/** Render the two closed-union helpers back to their original inline literals. */
const renderMigration = (text: string) => stripImport(text)
  .replaceAll('${PUBLICATION_ID_HEAD_SQL}', 'SELECT publication.id')
  .replaceAll("${ORPHAN_PRESERVATION_EVENT_HEAD_SQL('preserved')}", P1)
  .replaceAll("${ORPHAN_PRESERVATION_EVENT_HEAD_SQL('restored')}", P2)
  .replaceAll('${DISTINCT_SET_ROW_HEAD_SQL}', 'SELECT DISTINCT set_row.')
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('closed')}", "SELECT 1 FROM attachment_links closed WHERE closed.attachment_id=a.id AND closed.state IN (")
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('l')}", "SELECT 1 FROM attachment_links l WHERE l.attachment_id=a.id AND l.state IN (");
const renderRepository = (text: string) => stripImport(text)
  .replaceAll('db.prepare(SET_DRAFT_BOUND_COUNT_SQL(true))', `db.prepare("${H1}")`)
  .replaceAll('db.prepare(SET_DRAFT_BOUND_COUNT_SQL(false))', `db.prepare("${H2}")`)
  .replaceAll('${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL}', R47_A)
  .replaceAll('UPLOAD_INTENT_STAR_WHERE_HEAD_SQL + "', '"' + R49_A)
  .replaceAll('${DISTINCT_SET_ROW_HEAD_SQL}', 'SELECT DISTINCT set_row.')
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('closed')}", "SELECT 1 FROM attachment_links closed WHERE closed.attachment_id=a.id AND closed.state IN (")
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('l')}", "SELECT 1 FROM attachment_links l WHERE l.attachment_id=a.id AND l.state IN (");

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

describe('Files read-tail r46 byte-oracle dedupe', () => {
  test('the two closed-union heads are exact read-only byte oracles, distinct from r45', () => {
    expect(ORPHAN_PRESERVATION_EVENT_HEAD_SQL('preserved')).toBe(P1);
    expect(ORPHAN_PRESERVATION_EVENT_HEAD_SQL('restored')).toBe(P2);
    expect(SET_DRAFT_BOUND_COUNT_SQL(true)).toBe(H1);
    expect(SET_DRAFT_BOUND_COUNT_SQL(false)).toBe(H2);
    expect(HEADS.map(h => h.length)).toEqual([111, 110, 157, 148]);
    for (const head of HEADS) {
      expect(count(head, 'SELECT ')).toBe(1);
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // The alias union is the only difference; the rest of the count statement is shared.
    expect(H1).toBe(H2.replace('SELECT count(*) ', 'SELECT count(*) AS count '));
    expect(P1).not.toBe(P2);
    expect(P1).not.toBe(ORPHAN_PRESERVATION_PENDING_SQL);
    expect(ORPHAN_PRESERVATION_PENDING_SQL).toContain(P2);
    expect(P1).not.toBe(ATTACHMENT_EVENTS_WHERE_HEAD_SQL);
    expect(H2).not.toBe(ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL);
    const helper = src('adapters/read-tail-r46-source-sql.ts');
    for (const name of ['ORPHAN_PRESERVATION_EVENT_HEAD_SQL', 'SET_DRAFT_BOUND_COUNT_SQL']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
    expect(helper).toContain("e.event_kind='${kind}'");
    expect(helper).toContain("aliased ? 'AS count ' : ''");
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
  });

  test('no residual inline literal: each head lives only in its helper', () => {
    const residual: Record<string, number> = { P1: 0, P2: 0, H1: 0, H2: 0 };
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      for (const [name, head] of Object.entries({ P1, P2, H1, H2 })) residual[name] += count(text, head);
    }
    // P2 now lives only in the Rust-owned orphan-preservation JSON contract.
    expect(residual).toEqual({ P1: 0, P2: 0, H1: 0, H2: 0 });
    expect(src('adapters/orphan-preservation-head-sources.json')).toContain(P2);
    for (const prior of [
      'read-tail-dedupe-sql.ts', 'read-tail-r27-source-sql.ts', 'read-tail-r29-source-sql.ts',
      'read-tail-r31-source-sql.ts', 'read-tail-r42-source-sql.ts', 'read-tail-r44-source-sql.ts',
      'read-tail-r45-source-sql.ts', 'social-upload-cleanup.ts',
    ]) {
      expect(src(`adapters/${prior}`)).not.toContain(P1);
      expect(src(`adapters/${prior}`)).not.toContain(H2);
    }
  });

  test('rendered SQL is byte-identical to BASE; binds, locks and native writes unchanged', () => {
    for (const [rel, render] of [
      [MIG, renderMigration],
      [SR, renderRepository],
    ] as const) {
      const base = before(rel);
      const after = src(rel);
      expect(render(after)).toBe(base);
      for (const needle of ['.bind(', 'nativeWrite(', 'FOR UPDATE', 'FOR SHARE', 'FOR KEY SHARE', 'LOCK TABLE', 'pg_advisory_xact_lock']) {
        expect(count(after, needle)).toBe(count(base, needle));
      }
    }
    const sr = src(SR);
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('db.prepare(advisoryLockSql)');
    expect(src(MIG)).toContain("nativeWrite('MIG_001'");
  });

  test('each site composes its closed-union head and keeps every bind', () => {
    const mig = src(MIG);
    expect(count(mig, "${ORPHAN_PRESERVATION_EVENT_HEAD_SQL('preserved')}")).toBe(1);
    expect(count(mig, "${ORPHAN_PRESERVATION_EVENT_HEAD_SQL('restored')}")).toBe(1);
    expect(mig).toContain("WHERE EXISTS(${ORPHAN_PRESERVATION_EVENT_HEAD_SQL('preserved')})");
    expect(mig).toContain("AND NOT EXISTS(${ORPHAN_PRESERVATION_EVENT_HEAD_SQL('restored')})");
    const sr = src(SR);
    expect(count(sr, 'db.prepare(SET_DRAFT_BOUND_COUNT_SQL(true))')).toBe(1);
    expect(count(sr, 'db.prepare(SET_DRAFT_BOUND_COUNT_SQL(false))')).toBe(1);
    expect(count(sr, 'SET_DRAFT_BOUND_COUNT_SQL(true)).bind(facts.setId, facts.companyId).first("count")')).toBe(1);
    expect(count(sr, 'SET_DRAFT_BOUND_COUNT_SQL(false)).bind(facts.setId, facts.companyId).first("count")')).toBe(1);
    expect(sr).toContain("import {SET_DRAFT_BOUND_COUNT_SQL} from \"../read-tail-r46-source-sql\";");
  });

  test('retirement:ts-statements is 190, strictly below the r45 BEFORE=239 (239->237)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // Two closed-union families fold two sites into one helper each (-2):
    // the orphan-preservation event head and the draft/bound link count head.
    expect(literal).toBe(190);
    expect(total).toBe(191);
    expect(literal).toBeLessThan(239);
    expect(total).toBeLessThan(240);
  });
});
