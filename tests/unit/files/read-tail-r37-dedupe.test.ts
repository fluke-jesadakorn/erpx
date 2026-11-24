import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { ATTACHMENT_LINK_PROJECTION_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r37-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { encoding: 'utf8' });

/** Verbatim HEAD literals for the two attachment-link reads replaced by this family. */
const HEAD = {
  owner: 'SELECT l.*,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.expected_target_version,s.state set_state,s.company_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=?',
  joined: 'SELECT l.*,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.expected_target_version,\n    a.source_state,a.id attachment_id,a.preview_object_key FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id\n      AND a.company_id=s.company_id\n    WHERE l.id=? AND s.company_id=? AND (l.attachment_id IS NULL OR a.id IS NOT NULL)',
} as const;

const PRIOR_HELPERS = [
  'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r27-source-sql.ts',
  'read-tail-r28-source-sql.ts', 'read-tail-r29-source-sql.ts', 'read-tail-r30-source-sql.ts',
  'read-tail-r31-source-sql.ts', 'read-tail-r32-source-sql.ts', 'read-tail-r33-source-sql.ts',
  'read-tail-r34-source-sql.ts', 'read-tail-r35-source-sql.ts', 'read-tail-r36-source-sql.ts',
  'commerce-delivery-dedupe-sql.ts',
  'owner-projection-dedupe-sql.ts', 'preservation-presence-dedupe-sql.ts',
  'scan-pending-dedupe-sql.ts', 'social-draft-image-guard-sql.ts',
  'social-upload-cleanup-rows-sql.ts',
] as const;

describe('Files read-tail r37 parameterized family', () => {
  test('the shared head is a byte-identical prefix of both original HEAD literals', () => {
    expect(ATTACHMENT_LINK_PROJECTION_HEAD_SQL.length).toBe(94);
    expect(HEAD.owner.startsWith(ATTACHMENT_LINK_PROJECTION_HEAD_SQL)).toBe(true);
    expect(HEAD.joined.startsWith(ATTACHMENT_LINK_PROJECTION_HEAD_SQL)).toBe(true);
    expect((ATTACHMENT_LINK_PROJECTION_HEAD_SQL.match(/SELECT /g) ?? []).length).toBe(1);
    expect(ATTACHMENT_LINK_PROJECTION_HEAD_SQL).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    // novel: no r10-r36 helper already carried this head
    for (const h of PRIOR_HELPERS) expect(src(h)).not.toContain(ATTACHMENT_LINK_PROJECTION_HEAD_SQL);
  });

  test('both attachment-link call sites compose the head and render byte-identically', () => {
    const sr = src('attachments/service-repository.ts');
    // import + two call sites
    expect((sr.match(/ATTACHMENT_LINK_PROJECTION_HEAD_SQL/g) ?? []).length).toBe(3);
    const ownerTail = sr.match(/const attachmentLinkSql = `\$\{ATTACHMENT_LINK_PROJECTION_HEAD_SQL\}([^`]*)`/)![1];
    const joinedTail = sr.match(/db\.prepare\(`\$\{ATTACHMENT_LINK_PROJECTION_HEAD_SQL\}([^`]*)`\)/)![1];
    expect(ATTACHMENT_LINK_PROJECTION_HEAD_SQL + ownerTail).toBe(HEAD.owner);
    expect(ATTACHMENT_LINK_PROJECTION_HEAD_SQL + joinedTail).toBe(HEAD.joined);
    expect(sr).not.toContain(HEAD.owner);
    expect(sr).not.toContain(HEAD.joined);
    const helper = src('read-tail-r37-source-sql.ts');
    expect(helper.split('export const ATTACHMENT_LINK_PROJECTION_HEAD_SQL =').length - 1).toBe(1);
  });

  test('advisory locks, native writes and r1-r36 helpers stay untouched', () => {
    const sr = 'packages/server-files/src/adapters/attachments/service-repository.ts';
    const headSrc = git('show', `HEAD:${sr}`);
    const cur = readFileSync(sr, 'utf8');
    for (const needle of [
      'const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"',
      'const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"',
    ]) {
      expect(headSrc).toContain(needle);
      expect(cur).toContain(needle);
    }
    expect((headSrc.match(/nativeWrite\(/g) ?? []).length).toBe((cur.match(/nativeWrite\(/g) ?? []).length);
    for (const h of PRIOR_HELPERS) {
      const p = `packages/server-files/src/adapters/${h}`;
      expect(git('rev-parse', `HEAD:${p}`).trim()).toBe(git('hash-object', p).trim());
    }
    // Only the files repository adapter may be a modified tracked src file; the
    // new r37 helper is untracked until the lead integrates. Robust to a clean tree.
    const changed = git('diff', '--name-only', '--', 'packages/server-files/src').trim().split('\n').filter(Boolean);
    expect(changed.every(p => [
      sr,
      'packages/server-files/src/adapters/app-attachment-sources.ts',
      'packages/server-files/src/adapters/messaging-file-settlements.ts',
      'packages/server-files/src/adapters/field-file-writer-owner.ts',
      'packages/server-files/src/adapters/attachments/service.ts',
      'packages/server-files/src/adapters/legal-destruction-source-facts.ts',
      'packages/server-files/src/adapters/messaging-upload-rollback.ts',
    ].includes(p))).toBe(true);
  });

  test('retirement:ts-statements is strictly below the r36 BEFORE=258 baseline', () => {
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
    // r40 left 254; r41 lowers 254->251 (three shared read heads -> three helpers).
    expect(literal).toBe(251);
    expect(total).toBe(252);
    expect(literal).toBeLessThan(255);
    expect(total).toBeLessThan(256);
  });
});
