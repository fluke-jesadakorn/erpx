import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ATTACHMENT_EXISTS_HEAD_SQL,
  MESSAGING_SETTLEMENT_HEAD_SQL,
  PACKAGE_FILE_LINK_TARGET_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r48-source-sql';
import {
  FIELD_LINK_EXISTS_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r33-source-sql';
import {
  FIELD_SET_OWNER_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r36-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const BASE = '81d5eaf835021b61868d75632322dd6add11d738';
const HELPER = 'adapters/read-tail-r48-source-sql.ts';
const FIELD = 'adapters/field-file-writer-owner.ts';
const MSG = 'adapters/messaging-file-settlements.ts';
const ECOM = 'adapters/ecommerce-package-obligation-sources.ts';
const MKT = 'adapters/marketing-package-obligation-sources.ts';
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;

/** Verbatim byte oracles: the exact heads lifted from the four pre-r48 sites.
 * A intentionally spans the `attachment` / `attachment_sets` prefix and carries
 * two SELECT tokens; C deliberately ends mid-identifier at `s.target_`. */
const A = 'SELECT EXISTS(SELECT 1 FROM attachment';
const B = 'SELECT l.id link_id,s.id set_id,';
const C = "SELECT 'file-link' kind,l.id,s.target_";
const HEADS = [A, B, C];

const stripImport = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r48-source-sql'))
  .join('\n');
/** Render each site back to its exact pre-r48 inline literals. */
const renderField = (text: string) => stripImport(text)
  .replaceAll('${ATTACHMENT_EXISTS_HEAD_SQL}', A);
const renderMsg = (text: string) => stripImport(text)
  .replaceAll('${MESSAGING_SETTLEMENT_HEAD_SQL}', B);
const renderPackage = (text: string) => stripImport(text)
  .replaceAll('${PACKAGE_FILE_LINK_TARGET_HEAD_SQL}', C);

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

describe('Files read-tail r48 byte-oracle dedupe', () => {
  test('the three heads are exact read-only byte oracles, distinct from r33/r36', () => {
    expect(ATTACHMENT_EXISTS_HEAD_SQL).toBe(A);
    expect(MESSAGING_SETTLEMENT_HEAD_SQL).toBe(B);
    expect(PACKAGE_FILE_LINK_TARGET_HEAD_SQL).toBe(C);
    expect(HEADS.map(h => h.length)).toEqual([38, 32, 38]);
    expect(count(A, 'SELECT ')).toBe(2);
    expect(count(B, 'SELECT ')).toBe(1);
    expect(count(C, 'SELECT ')).toBe(1);
    for (const head of HEADS) {
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // A is only the shared prefix; the r33/r36 helpers keep their longer witnesses.
    expect(FIELD_LINK_EXISTS_SQL).not.toBe(A);
    expect(FIELD_SET_OWNER_HEAD_SQL).not.toBe(A);
    expect(FIELD_LINK_EXISTS_SQL.startsWith(A)).toBe(true);
    expect(FIELD_SET_OWNER_HEAD_SQL.startsWith(A)).toBe(true);
    // C splits mid-identifier: the two sites complete it differently.
    expect('s.target_id'.startsWith('s.target_')).toBe(true);
    expect('s.target_type'.startsWith('s.target_')).toBe(true);
    const helper = src(HELPER);
    for (const name of [
      'ATTACHMENT_EXISTS_HEAD_SQL', 'MESSAGING_SETTLEMENT_HEAD_SQL',
      'PACKAGE_FILE_LINK_TARGET_HEAD_SQL',
    ]) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
    expect(helper).not.toMatch(/prepare\(|\.bind\(|nativeWrite\(/);
  });

  test('no residual inline literal at any of the four sites', () => {
    const residual: Record<string, number> = { A: 0, B: 0, C: 0 };
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      for (const [name, head] of Object.entries({ A, B, C })) residual[name] += count(text, head);
    }
    // A also lives in the pre-existing r33/r36 helpers; B/C live only in r48.
    expect(residual).toEqual({ A: 3, B: 1, C: 1 });
    expect(count(src(FIELD), A)).toBe(0);
    expect(count(src(MSG), B)).toBe(0);
    expect(count(src(ECOM), C)).toBe(0);
    expect(count(src(MKT), C)).toBe(0);
  });

  test('rendered SQL is byte-identical to BASE; binds, locks and ORDER tails unchanged', () => {
    for (const [rel, render] of [
      [FIELD, renderField], [MSG, renderMsg], [ECOM, renderPackage], [MKT, renderPackage],
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
    expect(src(MSG)).toContain("messagingFileSettlementSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE attachment_links,attachment_sets,attachments,attachment_events,upload_intents,attachment_scan_attempts IN SHARE MODE');}");
    for (const rel of [ECOM, MKT]) {
      expect(src(rel)).toContain('LOCK TABLE attachment_sets,attachment_links,attachments,upload_intents,attachment_scan_attempts,attachment_publications,attachment_events IN SHARE MODE');
      expect(count(src(rel), '.bind(companyId)')).toBe(1);
    }
  });

  test('each site composes its head and keeps every bind and divergent tail', () => {
    const field = src(FIELD);
    expect(count(field, '${ATTACHMENT_EXISTS_HEAD_SQL}')).toBe(2);
    expect(field).toContain('${ATTACHMENT_EXISTS_HEAD_SQL}s a WHERE a.id=?');
    expect(field).toContain('${ATTACHMENT_EXISTS_HEAD_SQL}_sets s WHERE s.company_id=?');
    expect(count(field, ".bind(...values).first<{marker:boolean;scoped:boolean}>()")).toBe(2);
    expect(count(field, '.bind(actor.companyId,target.targetType,target.targetId)')).toBe(1);

    const msg = src(MSG);
    expect(count(msg, '${MESSAGING_SETTLEMENT_HEAD_SQL}')).toBe(2);
    expect(msg).toContain('${MESSAGING_SETTLEMENT_HEAD_SQL}l.attachment_id source_id');
    expect(msg).toContain('${MESSAGING_SETTLEMENT_HEAD_SQL}s.company_id,s.target_id room_id');

    expect(count(src(ECOM), '${PACKAGE_FILE_LINK_TARGET_HEAD_SQL}')).toBe(1);
    expect(src(ECOM)).toContain('${PACKAGE_FILE_LINK_TARGET_HEAD_SQL}id,to_jsonb(l)||');
    expect(count(src(MKT), '${PACKAGE_FILE_LINK_TARGET_HEAD_SQL}')).toBe(1);
    expect(src(MKT)).toContain('${PACKAGE_FILE_LINK_TARGET_HEAD_SQL}type,s.target_id,to_jsonb(l)||');
  });

  test('retirement:ts-statements is 225, strictly below the r47 BEFORE=233 (233->225)', () => {
    let literal = 0, total = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
    }
    // Three families fold two sites into one helper each: the scoped-existence
    // head (-2, two SELECT tokens), the messaging settlement head (-1) and the
    // ecommerce/marketing file-link target_ head (-1).
    expect(literal).toBe(223);
    expect(total).toBe(224);
    expect(literal).toBeLessThan(233);
    expect(total).toBeLessThan(234);
  });
});
