import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { ATTACHMENT_EVENT_EVENT_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r38-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const rel = (p: string) => `packages/server-files/src/adapters/${p}`;
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { cwd: rootDir, encoding: 'utf8' });

const HEAD = 'SELECT 1 FROM attachment_events event WHERE event.';
const PH = '${ATTACHMENT_EVENT_EVENT_HEAD_SQL}';

/** Three read-only witnesses replaced by this family. */
const SITES = [
  'app-attachment-sources.ts',
  'attachments/service-repository.ts',
  'messaging-file-settlements.ts',
] as const;

const PRIOR_HELPERS = [
  'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r27-source-sql.ts',
  'read-tail-r28-source-sql.ts', 'read-tail-r29-source-sql.ts', 'read-tail-r30-source-sql.ts',
  'read-tail-r31-source-sql.ts', 'read-tail-r32-source-sql.ts', 'read-tail-r33-source-sql.ts',
  'read-tail-r34-source-sql.ts', 'read-tail-r35-source-sql.ts', 'read-tail-r36-source-sql.ts',
  'read-tail-r37-source-sql.ts',
  'commerce-delivery-dedupe-sql.ts',
  'owner-projection-dedupe-sql.ts', 'preservation-presence-dedupe-sql.ts',
  'scan-pending-dedupe-sql.ts', 'social-draft-image-guard-sql.ts',
  'social-upload-cleanup-rows-sql.ts',
] as const;

describe('Files read-tail r38 parameterized family', () => {
  test('the shared head is a byte-identical prefix of all three HEAD statements', () => {
    expect(ATTACHMENT_EVENT_EVENT_HEAD_SQL).toBe(HEAD);
    expect((ATTACHMENT_EVENT_EVENT_HEAD_SQL.match(/SELECT /g) ?? []).length).toBe(1);
    expect(ATTACHMENT_EVENT_EVENT_HEAD_SQL).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const site of SITES) {
      const before = git('show', `HEAD:${rel(site)}`);
      expect(before).toContain(HEAD);
    }
    // novel: no r10-r37 helper already carried this head
    for (const h of PRIOR_HELPERS) expect(src(h)).not.toContain(HEAD);
  });

  test('all three call sites compose the head and render byte-identically', () => {
    for (const site of SITES) {
      const before = git('show', `HEAD:${rel(site)}`);
      const after = readFileSync(resolve(rootDir, rel(site)), 'utf8');
      const oi = before.indexOf(HEAD);
      const ci = after.indexOf(PH);
      expect(oi).toBeGreaterThan(-1);
      expect(ci).toBeGreaterThan(-1);
      // from the head to the closing backtick: the exact rendered statement text
      const original = before.slice(oi, before.indexOf('`', oi));
      const rendered = HEAD + after.slice(ci + PH.length, after.indexOf('`', ci));
      expect(rendered).toBe(original);
      // exactly one interpolation site per file (the import is not the placeholder)
      expect(after.split(PH).length - 1).toBe(1);
    }
    // no residual literal outside the single new helper
    let residual = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) residual += (readFileSync(path, 'utf8').split(HEAD).length - 1);
      }
    };
    walk(srcRoot);
    expect(residual).toBe(1);
    const helper = src('read-tail-r38-source-sql.ts');
    expect(helper.split('export const ATTACHMENT_EVENT_EVENT_HEAD_SQL =').length - 1).toBe(1);
  });

  test('advisory locks, row locks, native writes and r1-r37 helpers stay untouched', () => {
    const before = git('show', `HEAD:${rel('attachments/service-repository.ts')}`);
    const after = src('attachments/service-repository.ts');
    for (const needle of [
      'const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"',
      'const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"',
    ]) {
      expect(before).toContain(needle);
      expect(after).toContain(needle);
    }
    expect((before.match(/nativeWrite\(/g) ?? []).length).toBe((after.match(/nativeWrite\(/g) ?? []).length);
    for (const h of PRIOR_HELPERS) {
      const p = rel(h);
      expect(git('rev-parse', `HEAD:${p}`).trim()).toBe(git('hash-object', p).trim());
    }
    const changed = git('diff', '--name-only', '--', 'packages/server-files/src').trim().split('\n').filter(Boolean);
    expect(changed.every(p => [...SITES.map(s => rel(s)),
      rel('field-file-writer-owner.ts'), rel('attachments/service.ts'),
      rel('legal-destruction-source-facts.ts'), rel('messaging-upload-rollback.ts'),
          'packages/server-files/src/adapters/app-attachment-sources.ts',
      'packages/server-files/src/adapters/leave-review-evidence-facts.ts',
      'packages/server-files/src/adapters/attachments/migration.ts',
      'packages/server-files/src/adapters/attachments/service-repository.ts',
    ].includes(p))).toBe(true);
  });

  test('retirement:ts-statements is strictly below the r37 BEFORE=257 baseline', () => {
    let literal = 0;
    let total = 0;
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
    // r40 left 254; r41 lowers 254->251; r42 lowers 251->248 (three set-row JOIN sites -> one head, two ledger projection sites -> one head).
    expect(literal).toBe(248);
    expect(total).toBe(249);
    expect(literal).toBeLessThan(255);
    expect(total).toBeLessThan(256);
  });
});
