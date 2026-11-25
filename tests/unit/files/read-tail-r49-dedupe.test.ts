import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  SUM_BYTES_ATTACHMENT_HEAD_SQL,
  LEAVE_COUNT_HEAD_SQL,
  UPLOAD_INTENT_STAR_WHERE_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r49-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const BASE = '3e0e2620b59118d6312a06f67695545b0c4b7f6c';
const HELPER = 'adapters/read-tail-r49-source-sql.ts';
const BETA = 'adapters/beta-management-facts.ts';
const DOC = 'adapters/document-derivatives.ts';
const LEAVE = 'adapters/leave-review-evidence-facts.ts';
const SR = 'adapters/attachments/service-repository.ts';
const CLEANUP = 'adapters/social-upload-cleanup.ts';
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;

/** Verbatim byte oracles: B splits an identifier, D carries a newline and two
 * different following indents, A ends on the space after WHERE. */
const B = 'SELECT SUM(bytes) FROM attachment_';
const D = 'SELECT count(*)\n';
const A = 'SELECT * FROM upload_intents WHERE ';
const HEADS = [B, D, A];
const INDENT_1 = '            '; // 12 spaces, canonical count
const INDENT_2 = '                  '; // 18 spaces, legacy count

const stripImport = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r49-source-sql') && !line.includes('read-tail-r50-source-sql') && !line.includes('read-tail-r51-source-sql') && !line.includes('read-tail-r53-source-sql'))
  .join('\n')
  .replaceAll('SOURCE_STATE_HEAD_SQL + "', '"SELECT source_state');
/** Render each of the five sites back to its exact pre-r49 inline literal. */
const render = (text: string) => stripImport(text)
  .replaceAll('${SUM_BYTES_ATTACHMENT_HEAD_SQL}', B)
  .replaceAll('${LEAVE_COUNT_HEAD_SQL}', D)
  .replaceAll('${UPLOAD_INTENT_STAR_WHERE_HEAD_SQL}', A)
  .replaceAll('UPLOAD_INTENT_STAR_WHERE_HEAD_SQL + "', '"' + A)
  .replaceAll('${DISTINCT_SET_ROW_HEAD_SQL}', 'SELECT DISTINCT set_row.')
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('closed')}", "SELECT 1 FROM attachment_links closed WHERE closed.attachment_id=a.id AND closed.state IN (")
  .replaceAll("${LINK_STATE_EXISTS_HEAD_SQL('l')}", "SELECT 1 FROM attachment_links l WHERE l.attachment_id=a.id AND l.state IN (");

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

describe('Files read-tail r49 byte-oracle dedupe', () => {
  test('the three heads are exact read-only byte oracles with mid-identifier, newline and space splits', () => {
    expect(SUM_BYTES_ATTACHMENT_HEAD_SQL).toBe(B);
    expect(LEAVE_COUNT_HEAD_SQL).toBe(D);
    expect(UPLOAD_INTENT_STAR_WHERE_HEAD_SQL).toBe(A);
    expect(HEADS.map(h => h.length)).toEqual([34, 16, 35]);
    for (const head of HEADS) {
      expect(count(head, 'SELECT ')).toBe(1);
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // B completes differently at the two call sites (mid-identifier split).
    expect(B.endsWith('attachment_')).toBe(true);
    expect(('SELECT SUM(bytes) FROM attachment_derivatives').startsWith(B)).toBe(true);
    expect(('SELECT SUM(bytes) FROM attachment_storage_ledger').startsWith(B)).toBe(true);
    expect('attachment_derivatives').not.toBe('attachment_storage_ledger');
    // D keeps its trailing newline; the indentation of the next line differs.
    expect(D.endsWith('\n')).toBe(true);
    expect(count(D, '\n')).toBe(1);
    expect(INDENT_1).not.toBe(INDENT_2);
    expect(INDENT_1.length).toBe(12);
    expect(INDENT_2.length).toBe(18);
    // A is only the head; the WHERE tail completes differently at each site.
    expect(A.endsWith('WHERE ')).toBe(true);
    expect('id=? AND attachment_set_id IS NOT NULL').not.toBe("owner_type='social_draft' AND owner_id=?");
    const helper = src(HELPER);
    for (const name of [
      'SUM_BYTES_ATTACHMENT_HEAD_SQL', 'LEAVE_COUNT_HEAD_SQL',
      'UPLOAD_INTENT_STAR_WHERE_HEAD_SQL',
    ]) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
    expect(count(helper, 'SELECT count(*)\\n')).toBe(1);
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
  });

  test('no residual inline literal: each head lives only in its helper', () => {
    const residual: Record<string, number> = { B: 0, D: 0, A: 0 };
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      for (const [name, head] of Object.entries({ B, D, A })) residual[name] += count(text, head);
    }
    // D is stored escaped in its helper, so its runtime-newline form is nowhere.
    expect(residual).toEqual({ B: 1, D: 0, A: 1 });
    for (const rel of [BETA, DOC, LEAVE, SR, CLEANUP]) {
      const text = src(rel);
      expect(count(text, B)).toBe(0);
      expect(count(text, D)).toBe(0);
      expect(count(text, A)).toBe(0);
    }
  });

  test('rendered SQL is byte-identical to BASE; binds, locks and witness recheck unchanged', () => {
    for (const rel of [BETA, DOC, LEAVE, SR, CLEANUP]) {
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
    expect(sr).toContain('${uploadIntentForSetSql} FOR UPDATE');
    const cleanup = src(CLEANUP);
    expect(cleanup).toContain('LOCK TABLE attachments,upload_intents,attachment_links,attachment_sets,attachment_events,attachment_publications,audit_events IN SHARE ROW EXCLUSIVE MODE');
    expect(cleanup).toContain('export async function assertSocialUploadCleanupFiles');
    expect(cleanup).toContain('if(image(await graph(db,state.input))!==');
    // Strict newline + indentation byte oracles on the rendered leave statements.
    const leave = render(src(LEAVE));
    expect(leave).toContain(D + INDENT_1 + 'FROM attachment_sets set_row');
    expect(leave).toContain(D + INDENT_2 + 'FROM attachments source');
  });

  test('each site composes its head and keeps every bind and divergent tail', () => {
    const beta = src(BETA);
    expect(count(beta, '${SUM_BYTES_ATTACHMENT_HEAD_SQL}')).toBe(1);
    expect(beta).toContain('${SUM_BYTES_ATTACHMENT_HEAD_SQL}derivatives WHERE company_id=?');
    expect(beta).toContain('.bind(');

    const doc = src(DOC);
    expect(count(doc, '${SUM_BYTES_ATTACHMENT_HEAD_SQL}')).toBe(1);
    expect(doc).toContain('${SUM_BYTES_ATTACHMENT_HEAD_SQL}storage_ledger WHERE company_id=?');
    expect(doc).toContain('.bind(id,page,checksum)');

    const leave = src(LEAVE);
    expect(count(leave, '${LEAVE_COUNT_HEAD_SQL}')).toBe(2);
    expect(leave).toContain('${LEAVE_COUNT_HEAD_SQL}' + INDENT_1 + 'FROM attachment_sets set_row');
    expect(leave).toContain('${LEAVE_COUNT_HEAD_SQL}' + INDENT_2 + 'FROM attachments source');
    // Each guarded assertion still binds exactly three values.
    expect(count(leave, '[id, company_id, length],')).toBe(2);

    const sr = src(SR);
    expect(sr).toContain('UPLOAD_INTENT_STAR_WHERE_HEAD_SQL + "id=? AND attachment_set_id IS NOT NULL AND created_by=? AND company_id=?"');
    expect(sr).toContain('${uploadIntentForSetSql} FOR UPDATE');
    expect(sr).toContain('.bind(facts.uploadIntentId, facts.userId, facts.companyId)');

    const cleanup = src(CLEANUP);
    expect(count(cleanup, '${UPLOAD_INTENT_STAR_WHERE_HEAD_SQL}')).toBe(1);
    expect(cleanup).toContain('${UPLOAD_INTENT_STAR_WHERE_HEAD_SQL}owner_type=\'social_draft\' AND owner_id=?');
    expect(cleanup).toContain('.bind(input.id,input.id,input.id,input.companyId,input.id)');
  });

  test('retirement:ts-statements is 225, strictly below the r48 BEFORE=229 (229->225)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // B, D and A each fold two inline sites into one helper (-3 total).
    expect(literal).toBe(221);
    expect(total).toBe(222);
    expect(literal).toBeLessThan(229);
    expect(total).toBeLessThan(230);
  });
});
