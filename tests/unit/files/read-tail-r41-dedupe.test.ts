import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  FIELD_ATTACHMENT_OWNED_HEAD_SQL,
  FIELD_SET_MARKER_EXISTS_HEAD_SQL,
  STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r41-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (name: string) => git('show', `f3d32d13e47f8b3e4e39ebfcf5c55d8b7f966a7d:packages/server-files/src/adapters/${name}`);
const render = (text: string) => text
  .replaceAll('${FIELD_SET_MARKER_EXISTS_HEAD_SQL}', FIELD_SET_MARKER_EXISTS_HEAD_SQL)
  .replaceAll('${FIELD_ATTACHMENT_OWNED_HEAD_SQL}', FIELD_ATTACHMENT_OWNED_HEAD_SQL)
  .replaceAll('${STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL}', STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL);
const cap = (text: string, re: RegExp) => {
  const m = re.exec(text);
  expect(m).not.toBeNull();
  return m![1];
};

/** Verbatim byte oracles: the exact shared SQL heads extracted by r41. */
const H1 = ' OR EXISTS(SELECT 1 FROM attachment_links l LEFT JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id ';
const H2 = 'SELECT erpx_files_field_attachment_owned(?)';
const H3 = 'SELECT 1 FROM attachment_storage_ledger WHERE object_key=?';

const PRIOR_HELPERS = [
  'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r27-source-sql.ts',
  'read-tail-r28-source-sql.ts', 'read-tail-r29-source-sql.ts', 'read-tail-r30-source-sql.ts',
  'read-tail-r31-source-sql.ts', 'read-tail-r32-source-sql.ts', 'read-tail-r33-source-sql.ts',
  'read-tail-r34-source-sql.ts', 'read-tail-r35-source-sql.ts', 'read-tail-r36-source-sql.ts',
  'read-tail-r37-source-sql.ts', 'read-tail-r38-source-sql.ts',
  'read-tail-r40-source-sql.ts',
  'commerce-delivery-dedupe-sql.ts', 'owner-projection-dedupe-sql.ts',
  'preservation-presence-dedupe-sql.ts', 'scan-pending-dedupe-sql.ts',
  'social-draft-image-guard-sql.ts', 'social-upload-cleanup-rows-sql.ts',
] as const;

describe('Files read-tail r41 byte-oracle dedupe', () => {
  test('the three shared heads are exact read-only byte oracles', () => {
    expect(FIELD_SET_MARKER_EXISTS_HEAD_SQL).toBe(H1);
    expect(FIELD_ATTACHMENT_OWNED_HEAD_SQL).toBe(H2);
    expect(STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL).toBe(H3);
    for (const head of [FIELD_SET_MARKER_EXISTS_HEAD_SQL, FIELD_ATTACHMENT_OWNED_HEAD_SQL, STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL]) {
      expect((head.match(/SELECT /g) ?? []).length).toBe(1);
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // Novel: no prior helper carried any of these heads.
    for (const h of PRIOR_HELPERS) {
      for (const head of [H1, H2, H3]) expect(src(h)).not.toContain(head);
    }
    const helper = src('read-tail-r41-source-sql.ts');
    for (const name of ['FIELD_SET_MARKER_EXISTS_HEAD_SQL', 'FIELD_ATTACHMENT_OWNED_HEAD_SQL', 'STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL']) {
      expect(helper.split(`export const ${name} =`).length - 1).toBe(1);
    }
  });

  test('no residual literal: each head is inlined only in the new helper', () => {
    const residual: Record<string, number> = { H1: 0, H2: 0, H3: 0 };
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          residual.H1 += text.split(H1).length - 1;
          residual.H2 += text.split(H2).length - 1;
          residual.H3 += text.split(H3).length - 1;
        }
      }
    };
    walk(srcRoot);
    expect(residual).toEqual({ H1: 1, H2: 1, H3: 1 });
  });

  test('advisory locks, row locks, LOCK TABLE and nativeWrite stay untouched', () => {
    // Every touched tracked file keeps its advisory/data locks and native writes.
    const needles = ['pg_advisory_xact_lock', 'LOCK TABLE', 'FOR UPDATE', 'nativeWrite('];
    const changed = git('diff', '--name-only', '--', 'packages/server-files/src').trim().split('\n').filter(Boolean);
    for (const p of changed) {
      const after = readFileSync(resolve(rootDir, p), 'utf8');
      const b = git('show', `HEAD:${p}`);
      for (const needle of needles) expect(after.split(needle).length - 1).toBe(b.split(needle).length - 1);
    }
    // Five TS advisory-lock sites remain across the owner, none removed.
    let totalAfter = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) totalAfter += readFileSync(path, 'utf8').split('pg_advisory_xact_lock').length - 1;
      }
    };
    walk(srcRoot);
    expect(totalAfter).toBe(5);
    for (const h of PRIOR_HELPERS) {
      const p = `packages/server-files/src/adapters/${h}`;
      expect(git('rev-parse', `HEAD:${p}`).trim()).toBe(git('hash-object', p).trim());
    }
    expect(changed.every(p => [
      'packages/server-files/src/adapters/commerce-facts.ts',
      'packages/server-files/src/adapters/field-file-writer-owner.ts',
      'packages/server-files/src/adapters/attachments/service.ts',
      'packages/server-files/src/adapters/legal-destruction-source-facts.ts',
      'packages/server-files/src/adapters/messaging-upload-rollback.ts',
          'packages/server-files/src/adapters/app-attachment-sources.ts',
      'packages/server-files/src/adapters/leave-review-evidence-facts.ts',
      'packages/server-files/src/adapters/attachments/migration.ts',
      'packages/server-files/src/adapters/attachments/service-repository.ts',
      // r44 read-tail call sites and helper (assigned owner paths).
      'packages/server-files/src/adapters/read-tail-r44-source-sql.ts',
      'packages/server-files/src/adapters/collaboration-home-sources.ts',
      'packages/server-files/src/adapters/document-derivatives.ts',
      'packages/server-files/src/adapters/messaging-file-settlements.ts',
      'packages/server-files/src/adapters/portal-package-obligation-sources.ts',
      'packages/server-files/src/adapters/attachments/service.ts',
      'packages/server-files/src/adapters/attachments/service-repository.ts',
      'packages/server-files/src/adapters/messaging-upload-rollback.ts',
    ].includes(p))).toBe(true);
  });

  test('retirement:ts-statements is strictly below the r40 BEFORE=254 baseline (254->251->248->246->242)', () => {
    let literal = 0, total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          literal += (text.match(/SELECT /g) ?? []).length;
          total += (text.match(/SELECT\s/g) ?? []).length;
        }
      }
    };
    walk(srcRoot);
    // r42 left 248; r43 lowers 248->246 (two SELECT-bearing shared heads folded).
    // r44 lowers 246->242 (three more shared heads folded).
    expect(literal).toBe(242);
    expect(total).toBe(243);
    expect(literal).toBeLessThan(254);
    expect(total).toBeLessThan(255);
  });
});
