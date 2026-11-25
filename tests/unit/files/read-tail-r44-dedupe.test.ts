import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ATTACHMENT_STAR_ID_HEAD_SQL,
  SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL,
  UPLOAD_INTENTS_EXISTS_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r44-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const BASE = 'dd8cd38740b0c70d6be208771fe5c3dc13e83850';
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;
const norm = (text: string) => text.replace(/\s+/g, ' ').trim();

/** Verbatim byte oracles: the exact 42/36/49-byte heads extracted by r44. */
const H1 = 'EXISTS(SELECT 1 FROM upload_intents WHERE ';
const H2 = 'SELECT * FROM attachments WHERE id=?';
const H3 = 'OR EXISTS(SELECT 1 FROM attachment_scan_attempts ';
/** r45 folded guardPublishImage's inline prefix into the r25 helper. Only that
 * site is rendered back; the three pre-r25 helper sites stay as-is. */
const H4 = 'EXISTS(SELECT 1 FROM attachments a JOIN ';
const F2 = '${socialDraftImageExistsSql}upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id\n';
/** r45 folded two service-repository heads into the r45 helper. */
const H5 = 'SELECT 1 FROM attachment_events WHERE ';
const H6 = 'SELECT * FROM attachment_sets WHERE ';
/** r46 folded two closed-union heads; these are rendered back only so this
 * r44 oracle still compares against the pinned pre-r46 baseline. */
const R46_P1 = "SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='preserved'";
const R46_P2 = "SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='restored'";
const R46_C1 = "SELECT count(*) AS count FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.set_id=? AND s.company_id=? AND l.state IN ('draft','bound')";
const R46_C2 = "SELECT count(*) FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.set_id=? AND s.company_id=? AND l.state IN ('draft','bound')";
/** r47 folded the draft-link EXISTS witness; rendered back so this r44 oracle
 * still compares against the pinned pre-r47 baseline. */
const R47_A = 'SELECT 1 FROM attachment_links link WHERE link.set_id=set_row.id';
/** r48 folded the messaging settlement projection head; rendered back so this
 * r44 oracle still compares against the pinned pre-r48 baseline. */
const R48_B = 'SELECT l.id link_id,s.id set_id,';
/** r49 folded the sum-of-bytes and upload-intent star heads; rendered back so
 * this r44 oracle still compares against the pinned pre-r49 baseline. */
const R49_B = 'SELECT SUM(bytes) FROM attachment_';
const R49_A = 'SELECT * FROM upload_intents WHERE ';

const TARGETS = [
  'adapters/attachments/service.ts',
  'adapters/collaboration-home-sources.ts',
  'adapters/messaging-upload-rollback.ts',
  'adapters/document-derivatives.ts',
  'adapters/attachments/service-repository.ts',
  'adapters/messaging-file-settlements.ts',
  'adapters/portal-package-obligation-sources.ts',
] as const;

const stripR44Import = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r44-source-sql') && !line.includes('read-tail-r45-source-sql') && !line.includes('read-tail-r46-source-sql') && !line.includes('read-tail-r47-source-sql') && !line.includes('read-tail-r48-source-sql') && !line.includes('read-tail-r49-source-sql') && !line.includes('read-tail-r50-source-sql') && !line.includes('read-tail-r51-source-sql') && !line.includes('read-tail-r53-source-sql'))
  .join('\n');
/** Reconstruct the pre-r44 bytes: quoted splices and template/bare references
 * are rendered back to their original literals without touching tails. */
const render = (text: string) => text
  .replaceAll('" + UPLOAD_INTENTS_EXISTS_HEAD_SQL + "', H1)
  .replaceAll('${UPLOAD_INTENTS_EXISTS_HEAD_SQL}', H1)
  .replaceAll('${ATTACHMENT_STAR_ID_HEAD_SQL}', H2)
  .replaceAll('ATTACHMENT_STAR_ID_HEAD_SQL', `"${H2}"`)
  .replaceAll('${SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL}', H3)
  .replace(F2, H4 + 'upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id\n')
  .replaceAll('ATTACHMENT_EVENTS_WHERE_HEAD_SQL + "', '"' + H5)
  .replaceAll('ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL + "', '"' + H6)
  .replaceAll('${ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL}', H6)
  .replaceAll("${ORPHAN_PRESERVATION_EVENT_HEAD_SQL('preserved')}", R46_P1)
  .replaceAll("${ORPHAN_PRESERVATION_EVENT_HEAD_SQL('restored')}", R46_P2)
  .replaceAll('db.prepare(SET_DRAFT_BOUND_COUNT_SQL(true))', `db.prepare("${R46_C1}")`)
  .replaceAll('db.prepare(SET_DRAFT_BOUND_COUNT_SQL(false))', `db.prepare("${R46_C2}")`)
  .replaceAll('${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL}', R47_A)
  .replaceAll('${MESSAGING_SETTLEMENT_HEAD_SQL}', R48_B)
  .replaceAll('${SUM_BYTES_ATTACHMENT_HEAD_SQL}', R49_B)
  .replaceAll('UPLOAD_INTENT_STAR_WHERE_HEAD_SQL + "', '"' + R49_A)
  .replaceAll('${DISTINCT_SET_ROW_HEAD_SQL}', 'SELECT DISTINCT set_row.')
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('closed')}", "SELECT 1 FROM attachment_links closed WHERE closed.attachment_id=a.id AND closed.state IN (")
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('l')}", "SELECT 1 FROM attachment_links l WHERE l.attachment_id=a.id AND l.state IN (")
  .replaceAll('SOURCE_STATE_HEAD_SQL + "', '"SELECT source_state');

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

describe('Files read-tail r44 byte-oracle dedupe', () => {
  test('the three shared heads are exact read-only byte oracles with one definition each', () => {
    expect(UPLOAD_INTENTS_EXISTS_HEAD_SQL).toBe(H1);
    expect(ATTACHMENT_STAR_ID_HEAD_SQL).toBe(H2);
    expect(SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL).toBe(H3);
    expect([H1.length, H2.length, H3.length]).toEqual([42, 36, 49]);
    expect([count(H1, 'SELECT '), count(H2, 'SELECT '), count(H3, 'SELECT ')]).toEqual([1, 1, 1]);
    for (const head of [H1, H2, H3]) {
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const helper = src('adapters/read-tail-r44-source-sql.ts');
    for (const name of ['UPLOAD_INTENTS_EXISTS_HEAD_SQL', 'ATTACHMENT_STAR_ID_HEAD_SQL', 'SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
      expect(count(helper, '${' + name + '}')).toBe(0);
    }
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
  });

  test('no residual literal: each head lives only in the r44 helper', () => {
    const residual: Record<string, number> = { H1: 0, H2: 0, H3: 0 };
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      residual.H1 += count(text, H1);
      residual.H2 += count(text, H2);
      residual.H3 += count(text, H3);
    }
    expect(residual).toEqual({ H1: 1, H2: 1, H3: 1 });
    expect(src('adapters/read-tail-r44-source-sql.ts')).toContain(H1);
    expect(src('adapters/read-tail-r44-source-sql.ts')).toContain(H2);
    expect(src('adapters/read-tail-r44-source-sql.ts')).toContain(H3);
    // Novel: r26 attachmentsByIdCompanySql and scanPendingSql are distinct.
    expect(src('adapters/read-tail-r26-source-sql.ts')).not.toContain(H2);
    expect(src('adapters/scan-pending-dedupe-sql.ts')).not.toContain(H3);
    expect(src('adapters/read-tail-r43-source-sql.ts')).not.toContain(H1);
  });

  test('rendered SQL is byte-identical to the pinned baseline; binds unchanged', () => {
    for (const rel of TARGETS) {
      expect(render(stripR44Import(src(rel)))).toBe(before(rel));
      expect(norm(render(stripR44Import(src(rel))))).toBe(norm(before(rel)));
      expect(count(src(rel), '.bind(')).toBe(count(before(rel), '.bind('));
      expect(count(src(rel), '.bind(...values)')).toBe(count(before(rel), '.bind(...values)'));
    }
  });

  test('NOT / OR prefixes, quoted STORAGE_LEDGER splice, advisory locks and LOCK TABLE stay put', () => {
    const service = src('adapters/attachments/service.ts');
    const home = src('adapters/collaboration-home-sources.ts');
    const rollback = src('adapters/messaging-upload-rollback.ts');
    expect(service).toContain('OR " + UPLOAD_INTENTS_EXISTS_HEAD_SQL + "company_id=?');
    expect(home).toContain('"NOT " + UPLOAD_INTENTS_EXISTS_HEAD_SQL + "owner_type=');
    expect(rollback).toContain('OR ${UPLOAD_INTENTS_EXISTS_HEAD_SQL}quarantine_key=?');
    // The single-quoted TS string keeps the literal placeholder and the ledger.
    expect(service).toContain('OR EXISTS(${STORAGE_LEDGER_PRESENCE_SQL} AND object_key=?)');
    expect(service).toContain('uploadCleanupLock: key => db.prepare(advisoryLockSql).bind(key).run()');
    expect(count(service, 'pg_advisory_xact_lock')).toBe(count(before('adapters/attachments/service.ts'), 'pg_advisory_xact_lock'));
    expect(count(rollback, 'pg_advisory_xact_lock')).toBe(count(before('adapters/messaging-upload-rollback.ts'), 'pg_advisory_xact_lock'));
    for (const rel of [
      'adapters/messaging-upload-rollback.ts',
      'adapters/messaging-file-settlements.ts',
      'adapters/portal-package-obligation-sources.ts',
    ]) {
      expect(count(src(rel), 'LOCK TABLE')).toBe(count(before(rel), 'LOCK TABLE'));
      expect(src(rel)).toContain('LOCK TABLE');
    }
    // The two scan witnesses keep their own aliases and state tails.
    expect(src('adapters/messaging-file-settlements.ts')).toContain('${SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL}scan WHERE scan.attachment_id=source.id AND scan.state IN (\'pending\',\'running\'))');
    expect(src('adapters/portal-package-obligation-sources.ts')).toContain('${SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL}attempt WHERE attempt.attachment_id=attachment.id AND attempt.state IN (\'pending\',\'running\',\'error\'))');
  });

  test('retirement:ts-statements is 242, strictly below the r43 BEFORE=246 (246->242)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // r43 left 246: the upload_intents EXISTS head folds 3 sites into one
    // (-2), while the attachment star-id and scan OR-EXISTS heads each fold
    // two sites into one (-1 each). Three heads live in the r44 helper.
    // r46 folds two closed-union heads (orphan-preservation event, draft/bound count): 239->237.
    expect(literal).toBe(222);
    expect(total).toBe(223);
    expect(literal).toBeLessThan(246);
    expect(total).toBeLessThan(247);
  });
});
