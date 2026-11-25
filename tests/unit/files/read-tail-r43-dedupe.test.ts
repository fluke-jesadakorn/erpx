import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ATTACHMENT_ID_AND_HEAD_SQL,
  FIELD_SET_UPLOAD_EXISTS_HEAD_SQL,
  FIELD_UPLOAD_ANCESTOR_EXISTS_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r43-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const BASE = '337fd9466e26fda35934822c9ea216249a522850';
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (rel: string) => git('show', `${BASE}:packages/server-files/src/${rel}`);
const count = (text: string, needle: string) => text.split(needle).length - 1;
const norm = (text: string) => text.replace(/\s+/g, ' ').trim();

/** Verbatim byte oracles: the exact shared SQL heads extracted by r43. */
const H1 = 'EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND erpx_files_field_attachment_owned(a.id))';
const H2 = 'OR EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=s.id AND ';
const H3 = 'FROM attachments a WHERE a.id=? AND ';
/** r48 folded the scoped-existence prefix at the Field attachment site; the
 * H3 substring now lives only in the helper and the legal-preservation site. */
const R48_A = 'SELECT EXISTS(SELECT 1 FROM attachment';

const TARGETS = [
  'adapters/field-file-writer-owner.ts',
  'adapters/commerce-facts.ts',
] as const;

/** The id/AND head stays literal at the two sites with a different tail. */
const UNFOLDED_H3 = [
  'adapters/legal-preservation-source-facts.ts',
] as const;

const PRIOR_HELPERS = [
  'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r27-source-sql.ts',
  'read-tail-r28-source-sql.ts', 'read-tail-r29-source-sql.ts', 'read-tail-r30-source-sql.ts',
  'read-tail-r31-source-sql.ts', 'read-tail-r32-source-sql.ts', 'read-tail-r33-source-sql.ts',
  'read-tail-r34-source-sql.ts', 'read-tail-r35-source-sql.ts', 'read-tail-r36-source-sql.ts',
  'read-tail-r37-source-sql.ts', 'read-tail-r38-source-sql.ts', 'read-tail-r40-source-sql.ts',
  'read-tail-r41-source-sql.ts', 'read-tail-r42-source-sql.ts',
  'commerce-delivery-dedupe-sql.ts',
  'social-draft-image-guard-sql.ts', 'social-upload-cleanup-rows-sql.ts',
] as const;

const stripR43Import = (text: string) => text
  .split('\n')
  .filter(line => !line.includes('read-tail-r43-source-sql') && !line.includes('read-tail-r48-source-sql'))
  .join('\n');
const render = (text: string) => text
  .replaceAll('${FIELD_UPLOAD_ANCESTOR_EXISTS_SQL}', H1)
  .replaceAll('${FIELD_SET_UPLOAD_EXISTS_HEAD_SQL}', H2)
  .replaceAll('${ATTACHMENT_ID_AND_HEAD_SQL}', H3)
  .replaceAll('${ATTACHMENT_EXISTS_HEAD_SQL}', R48_A);

describe('Files read-tail r43 byte-oracle dedupe', () => {
  test('the three shared heads are exact read-only byte oracles with one definition each', () => {
    expect(FIELD_UPLOAD_ANCESTOR_EXISTS_SQL).toBe(H1);
    expect(FIELD_SET_UPLOAD_EXISTS_HEAD_SQL).toBe(H2);
    expect(ATTACHMENT_ID_AND_HEAD_SQL).toBe(H3);
    expect(count(H1, 'SELECT ')).toBe(1);
    expect(count(H2, 'SELECT ')).toBe(1);
    expect(count(H3, 'SELECT ')).toBe(0); // the two projections stay at their call sites
    for (const head of [H1, H2, H3]) {
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // Novel: no prior helper carried any of these heads.
    for (const h of PRIOR_HELPERS) {
      const prior = src(`adapters/${h}`);
      expect(prior).not.toContain(H1);
      expect(prior).not.toContain(H2);
      expect(prior).not.toContain(H3);
    }
    const helper = src('adapters/read-tail-r43-source-sql.ts');
    for (const name of ['FIELD_UPLOAD_ANCESTOR_EXISTS_SQL', 'FIELD_SET_UPLOAD_EXISTS_HEAD_SQL', 'ATTACHMENT_ID_AND_HEAD_SQL']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
      expect(count(helper, '${' + name + '}')).toBe(0);
    }
  });

  test('no residual literal: H1 and H2 live only in the helper; H3 keeps its two different-tail sites', () => {
    const residual: Record<string, number> = { H1: 0, H2: 0, H3: 0 };
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          residual.H1 += count(text, H1);
          residual.H2 += count(text, H2);
          residual.H3 += count(text, H3);
        }
      }
    };
    walk(srcRoot);
    expect(residual).toEqual({ H1: 1, H2: 1, H3: 2 });
    for (const rel of UNFOLDED_H3) expect(src(rel)).toContain(H3);
    expect(count(src('adapters/field-file-writer-owner.ts'), H1)).toBe(0);
    expect(count(src('adapters/field-file-writer-owner.ts'), H2)).toBe(0);
    expect(count(src('adapters/commerce-facts.ts'), H3)).toBe(0);
  });

  test('rendered SQL is byte-identical to the pinned baseline; binds unchanged', () => {
    for (const rel of TARGETS) {
      expect(render(stripR43Import(src(rel)))).toBe(before(rel));
      expect(norm(render(stripR43Import(src(rel))))).toBe(norm(before(rel)));
    }
    // Field owner bind arrays and commerce binds stay unchanged.
    expect(count(src('adapters/field-file-writer-owner.ts'), '.bind(...values)')).toBe(
      count(before('adapters/field-file-writer-owner.ts'), '.bind(...values)'));
    expect(count(src('adapters/commerce-facts.ts'), '.bind(')).toBe(
      count(before('adapters/commerce-facts.ts'), '.bind('));
    // The r41 marker is still interpolated by the shared-target preflight.
    expect(src('adapters/field-file-writer-owner.ts')).toContain('${fieldSetMarker}');
  });

  test('advisory locks, row locks, LOCK TABLE and nativeWrite stay untouched', () => {
    const needles = ['pg_advisory_xact_lock', 'LOCK TABLE', 'FOR UPDATE', 'nativeWrite('];
    for (const rel of TARGETS) {
      const after = src(rel);
      const b = before(rel);
      for (const needle of needles) expect(count(after, needle)).toBe(count(b, needle));
    }
    let totalAfter = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) totalAfter += count(readFileSync(path, 'utf8'), 'pg_advisory_xact_lock');
      }
    };
    walk(srcRoot);
    expect(totalAfter).toBe(5);
    // scan-pending-, owner-projection- and preservation-presence-dedupe-sql.ts
    // are excluded: their statement tokens moved to native-owned JSON contracts
    // (see the corresponding *-native-head suites), so their byte hashes
    // legitimately changed.
    for (const h of PRIOR_HELPERS) {
      const p = `packages/server-files/src/adapters/${h}`;
      expect(git('rev-parse', `${BASE}:${p}`).trim()).toBe(git('hash-object', p).trim());
    }
  });

  test('retirement:ts-statements is 195, strictly below the r42 BEFORE=248 (248->246->242)', () => {
    let literal = 0, total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          literal += count(text, 'SELECT ');
          total += (text.match(/SELECT\s/g) ?? []).length;
        }
      }
    };
    walk(srcRoot);
    // r42 left 248. The Field upload-ancestor and upload_intents OR heads each
    // carry one SELECT and are folded from two sites (-1 each). The id/AND head
    // carries no SELECT and both commerce sites keep their projections (0).
    // r44 lowers 246->242 (three more shared heads folded).
    // r46 folds two closed-union heads (orphan-preservation event, draft/bound count): 239->237.
    // Native lanes 21+22 retire inlined statements from the owner tree; lane 21
    // wave 2 also retires the two finance-tax-signature reads. Pins equal the
    // current worktree count: 221->211 (spaced) and 222->212 (bare).
    expect(literal).toBe(195);
    expect(total).toBe(196);
    expect(literal).toBeLessThan(248);
    expect(total).toBeLessThan(249);
  });
});
