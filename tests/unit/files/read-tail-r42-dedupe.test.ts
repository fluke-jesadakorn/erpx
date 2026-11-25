import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  LEDGER_OBJECT_KEY_PROJECTION_HEAD_SQL,
  SET_ROW_JOIN_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r42-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });
const before = (name: string) => git('show', `c082ac46210ca2511a5aaa087e0c942cbc487d6f:packages/server-files/src/adapters/${name}`);
const render = (text: string) => text
  .replaceAll('${SET_ROW_JOIN_HEAD_SQL}', SET_ROW_JOIN_HEAD_SQL)
  .replaceAll('${LEDGER_OBJECT_KEY_PROJECTION_HEAD_SQL}', LEDGER_OBJECT_KEY_PROJECTION_HEAD_SQL);
const cap = (text: string, re: RegExp) => {
  const m = re.exec(text);
  expect(m).not.toBeNull();
  return m![1];
};
const count = (text: string, needle: string) => text.split(needle).length - 1;
const norm = (text: string) => text.replace(/\s+/g, ' ').trim();
const tailFrom = (text: string, marker: string) => text.slice(text.indexOf(marker));

/** Verbatim byte oracles: the exact shared SQL heads extracted by r42. */
const H1 = 'SELECT 1 FROM attachment_sets set_row JOIN ';
const H2 = 'SELECT object_key FROM attachment_storage_ledger WHERE ';
/** The distinct r41 object_key=? presence head stays at its own sites. */
const R41_OBJECT_KEY_PRESENCE_HEAD = 'SELECT 1 FROM attachment_storage_ledger WHERE object_key=?';

const PRIOR_HELPERS = [
  'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r27-source-sql.ts',
  'read-tail-r28-source-sql.ts', 'read-tail-r29-source-sql.ts', 'read-tail-r30-source-sql.ts',
  'read-tail-r31-source-sql.ts', 'read-tail-r32-source-sql.ts', 'read-tail-r33-source-sql.ts',
  'read-tail-r34-source-sql.ts', 'read-tail-r35-source-sql.ts', 'read-tail-r36-source-sql.ts',
  'read-tail-r37-source-sql.ts', 'read-tail-r38-source-sql.ts',
  'read-tail-r40-source-sql.ts', 'read-tail-r41-source-sql.ts',
  'commerce-delivery-dedupe-sql.ts', 'owner-projection-dedupe-sql.ts',
  'preservation-presence-dedupe-sql.ts', 'scan-pending-dedupe-sql.ts',
  'social-draft-image-guard-sql.ts', 'social-upload-cleanup-rows-sql.ts',
] as const;

const helpers = (p: string) => [`packages/server-files/src/adapters/${p}`];

describe('Files read-tail r42 byte-oracle dedupe', () => {
  test('both shared heads are exact read-only byte oracles with one definition each', () => {
    expect(SET_ROW_JOIN_HEAD_SQL).toBe(H1);
    expect(LEDGER_OBJECT_KEY_PROJECTION_HEAD_SQL).toBe(H2);
    for (const head of [H1, H2]) {
      expect(count(head, 'SELECT ')).toBe(1);
      expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // The ledger projection head is distinct from the r41 object_key=? head.
    expect(H2).not.toBe(R41_OBJECT_KEY_PRESENCE_HEAD);
    expect(src('read-tail-r41-source-sql.ts')).toContain(R41_OBJECT_KEY_PRESENCE_HEAD);
    // Novel: no prior helper carried either head.
    for (const h of PRIOR_HELPERS) {
      expect(src(h)).not.toContain(H1);
      expect(src(h)).not.toContain(H2);
    }
    const helper = src('read-tail-r42-source-sql.ts');
    for (const name of ['SET_ROW_JOIN_HEAD_SQL', 'LEDGER_OBJECT_KEY_PROJECTION_HEAD_SQL']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
  });

  test('no residual literal: each head is inlined only in the new helper', () => {
    const residual: Record<string, number> = { H1: 0, H2: 0 };
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          residual.H1 += count(text, H1);
          residual.H2 += count(text, H2);
        }
      }
    };
    walk(srcRoot);
    expect(residual).toEqual({ H1: 1, H2: 1 });
  });

  test('advisory locks, row locks, LOCK TABLE and nativeWrite stay untouched', () => {
    const needles = ['pg_advisory_xact_lock', 'LOCK TABLE', 'FOR UPDATE', 'nativeWrite('];
    const changed = git('diff', '--name-only', '--', 'packages/server-files/src').trim().split('\n').filter(Boolean);
    for (const p of changed) {
      const after = readFileSync(resolve(rootDir, p), 'utf8');
      const b = git('show', `HEAD:${p}`);
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
    for (const h of PRIOR_HELPERS) {
      for (const p of helpers(h)) {
        expect(git('rev-parse', `HEAD:${p}`).trim()).toBe(git('hash-object', p).trim());
      }
    }
    expect(changed.every(p => [
      'packages/server-files/src/adapters/commerce-facts.ts',
      'packages/server-files/src/adapters/field-file-writer-owner.ts',
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
      // r47 read-tail call site (assigned owner path).
      'packages/server-files/src/adapters/social-upload-cleanup.ts',
      // r48 read-tail call sites and helper (assigned owner paths).
      'packages/server-files/src/adapters/ecommerce-package-obligation-sources.ts',
      'packages/server-files/src/adapters/marketing-package-obligation-sources.ts',
      'packages/server-files/src/adapters/read-tail-r48-source-sql.ts',
      // r49 read-tail call site (assigned owner path).
      'packages/server-files/src/adapters/beta-management-facts.ts',
      // r50 read-tail call sites and helper (assigned owner paths).
      'packages/server-files/src/adapters/leave-evidence-sources.ts',
      'packages/server-files/src/adapters/read-tail-r50-source-sql.ts',
      // r51 read-tail call sites and helper (assigned owner paths).
      'packages/server-files/src/adapters/read-tail-r51-source-sql.ts',
      // r52 read-tail call sites and helper (assigned owner paths).
      'packages/server-files/src/adapters/read-tail-r52-source-sql.ts',
      'packages/server-files/src/adapters/attachment-denied-read-audits.ts',
      'packages/server-files/src/adapters/customer-portal-files.ts',
      // r53 read-tail call site and helper (assigned owner paths).
      'packages/server-files/src/adapters/attachments/service-repository.ts',
      'packages/server-files/src/adapters/read-tail-r53-source-sql.ts',
      // lane 21 + 22 native cutovers: call sites and per-holding module export.
      'packages/server-files/src/adapters/commercial-usage-facts.ts',
      'packages/server-files/src/adapters/native_lane_21/mod.rs',
      'packages/server-files/src/adapters/work-read-facts.ts',
      // wave 2 native cutovers: retired call sites and owner modules.
      'packages/server-files/src/adapters/finance-tax-signatures.ts',
      'packages/server-files/src/adapters/people-evaluation-facts.ts',
      'packages/server-files/src/adapters/records-employee-requests-owned-facts.ts',
    ].includes(p) || /native_lane_\d+|native-lane-\d+/.test(p))).toBe(true);
  });

  test('retirement:ts-statements is 210, strictly below the r42 BEFORE=248 (248->246->242)', () => {
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
    // r46 folds two closed-union heads (orphan-preservation event, draft/bound count): 239->237.
    expect(literal).toBe(210);
    expect(total).toBe(211);
    expect(literal).toBeLessThan(248);
    expect(total).toBeLessThan(249);
  });
});
