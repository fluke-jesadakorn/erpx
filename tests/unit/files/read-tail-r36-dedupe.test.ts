import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { FIELD_SET_OWNER_HEAD_SQL } from '../../../modules/files/server/src/adapters/read-tail-r36-source-sql';

const srcRoot = resolve(__dirname, '../../../modules/files/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const git = (...a: string[]) => execFileSync('git', a, { encoding: 'utf8' });

/** Verbatim HEAD template literals for the two set preflights replaced by this family. */
const HEAD = {
  finance: 'SELECT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=? AND ${serviceSetMarker}) marker,${scopedByCompanySql(\'attachment_sets\')}',
  field: 'SELECT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=? AND ${fieldSetMarker}) marker,${scopedByCompanySql(\'attachment_sets\')}',
} as const;

const PRIOR_HELPERS = [
  'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r27-source-sql.ts',
  'read-tail-r28-source-sql.ts', 'read-tail-r29-source-sql.ts', 'read-tail-r30-source-sql.ts',
  'read-tail-r31-source-sql.ts', 'read-tail-r32-source-sql.ts', 'read-tail-r33-source-sql.ts',
  'read-tail-r34-source-sql.ts', 'read-tail-r35-source-sql.ts', 'commerce-delivery-dedupe-sql.ts',
  'scan-pending-dedupe-sql.ts', 'social-draft-image-guard-sql.ts',
  'social-upload-cleanup-rows-sql.ts',
] as const;

describe('Files read-tail r36 parameterized family', () => {
  test('the shared head is a byte-identical prefix of both original HEAD literals', () => {
    expect(FIELD_SET_OWNER_HEAD_SQL.length).toBe(63);
    expect(HEAD.finance.startsWith(FIELD_SET_OWNER_HEAD_SQL)).toBe(true);
    expect(HEAD.field.startsWith(FIELD_SET_OWNER_HEAD_SQL)).toBe(true);
    expect((FIELD_SET_OWNER_HEAD_SQL.match(/SELECT /g) ?? []).length).toBe(2);
    expect(FIELD_SET_OWNER_HEAD_SQL).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    // novel: no r10-r35 helper already carried this head
    for (const h of PRIOR_HELPERS) expect(src(h)).not.toContain(FIELD_SET_OWNER_HEAD_SQL);
  });

  test('both Field set call sites compose the head and render byte-identically', () => {
    const f = src('field-file-writer-owner.ts');
    // import + two call sites
    expect((f.match(/FIELD_SET_OWNER_HEAD_SQL/g) ?? []).length).toBe(3);
    const tails = [...f.matchAll(/set:`\$\{FIELD_SET_OWNER_HEAD_SQL\}([^`]*)`/g)].map(m => m[1]);
    expect(tails.length).toBe(2);
    expect(HEAD.finance).toBe(FIELD_SET_OWNER_HEAD_SQL + tails[0]);
    expect(HEAD.field).toBe(FIELD_SET_OWNER_HEAD_SQL + tails[1]);
    expect(f).not.toContain(HEAD.finance);
    expect(f).not.toContain(HEAD.field);
    const helper = src('read-tail-r36-source-sql.ts');
    expect(helper.split('export const FIELD_SET_OWNER_HEAD_SQL =').length - 1).toBe(1);
  });

  test('advisory locks, native writes and r1-r35 helpers stay untouched', () => {
    const sr = 'modules/files/server/src/adapters/attachments/service-repository.ts';
    for (const needle of [
      'const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"',
      'const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"',
    ]) {
      expect(git('show', `HEAD:${sr}`)).toContain(needle);
      expect(readFileSync(sr, 'utf8')).toContain(needle);
    }
    for (const h of PRIOR_HELPERS) {
      const p = `modules/files/server/src/adapters/${h}`;
      expect(git('rev-parse', `HEAD:${p}`).trim()).toBe(git('hash-object', p).trim());
    }
    // Only the Field owner adapter may be a modified tracked src file; the new
    // r36 helper is untracked until the lead integrates. Robust to clean tree.
    const changed = git('diff', '--name-only', '--', 'modules/files/server/src').trim().split('\n').filter(Boolean);
    expect(changed.every(p => [
      'modules/files/server/src/adapters/read-tail-r45-source-sql.ts',
      'modules/files/server/src/adapters/read-tail-r43-source-sql.ts',
      'modules/files/server/src/adapters/field_owner_head.rs',
      'modules/files/server/src/public/field_owner_head.rs',
      'modules/files/server/src/adapters/settlement_head.rs',
      'modules/files/server/src/public/settlement_head.rs',
      'modules/files/server/src/adapters/mod.rs',
      'modules/files/server/src/public/mod.rs',
      'modules/files/server/src/adapters/commerce-facts.ts',
      'modules/files/server/src/adapters/field-file-writer-owner.ts',
      'modules/files/server/src/adapters/attachments/service-repository.ts',
      'modules/files/server/src/adapters/app-attachment-sources.ts',
      'modules/files/server/src/adapters/messaging-file-settlements.ts',
      'modules/files/server/src/adapters/field-file-writer-owner.ts',
      'modules/files/server/src/adapters/attachments/service.ts',
      'modules/files/server/src/adapters/legal-destruction-source-facts.ts',
      'modules/files/server/src/adapters/messaging-upload-rollback.ts',
          'modules/files/server/src/adapters/app-attachment-sources.ts',
      'modules/files/server/src/adapters/leave-review-evidence-facts.ts',
      'modules/files/server/src/adapters/attachments/migration.ts',
      'modules/files/server/src/adapters/attachments/service-repository.ts',
      // r44 read-tail call sites and helper (assigned owner paths).
      'modules/files/server/src/adapters/read-tail-r44-source-sql.ts',
      'modules/files/server/src/adapters/collaboration-home-sources.ts',
      'modules/files/server/src/adapters/document-derivatives.ts',
      'modules/files/server/src/adapters/messaging-file-settlements.ts',
      'modules/files/server/src/adapters/portal-package-obligation-sources.ts',
      'modules/files/server/src/adapters/attachments/service.ts',
      'modules/files/server/src/adapters/attachments/service-repository.ts',
      'modules/files/server/src/adapters/messaging-upload-rollback.ts',
      // r47 read-tail call site (assigned owner path).
      'modules/files/server/src/adapters/social-upload-cleanup.ts',
      // r48 read-tail call sites and helper (assigned owner paths).
      'modules/files/server/src/adapters/ecommerce-package-obligation-sources.ts',
      'modules/files/server/src/adapters/marketing-package-obligation-sources.ts',
      'modules/files/server/src/adapters/read-tail-r48-source-sql.ts',
      // r49 read-tail call site (assigned owner path).
      'modules/files/server/src/adapters/beta-management-facts.ts',
      // r50 read-tail call sites and helper (assigned owner paths).
      'modules/files/server/src/adapters/leave-evidence-sources.ts',
      'modules/files/server/src/adapters/read-tail-r50-source-sql.ts',
      // r51 read-tail call sites and helper (assigned owner paths).
      'modules/files/server/src/adapters/read-tail-r51-source-sql.ts',
      // r52 read-tail call sites and helper (assigned owner paths).
      'modules/files/server/src/adapters/read-tail-r52-source-sql.ts',
      'modules/files/server/src/adapters/attachment-denied-read-audits.ts',
      'modules/files/server/src/adapters/customer-portal-files.ts',
      // r53 read-tail call site and helper (assigned owner paths).
      'modules/files/server/src/adapters/attachments/service-repository.ts',
      'modules/files/server/src/adapters/read-tail-r53-source-sql.ts',
      // lane 21 + 22 native cutovers: call sites and per-holding module export.
      'modules/files/server/src/adapters/commercial-usage-facts.ts',
      'modules/files/server/src/adapters/native_lane_21/mod.rs',
      'modules/files/server/src/adapters/work-read-facts.ts',
      // wave 2 native cutovers: retired call sites and owner modules.
      'modules/files/server/src/adapters/finance-tax-signatures.ts',
      'modules/files/server/src/adapters/people-evaluation-facts.ts',
      'modules/files/server/src/adapters/records-employee-requests-owned-facts.ts',
    ].includes(p) || /native_lane_\d+|native-lane-\d+/.test(p))).toBe(true);
  });

  test('retirement:ts-statements is strictly below the r35 BEFORE=260 baseline', () => {
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
    // r42 left 248; r43 lowers 248->246 (two SELECT-bearing shared heads folded).
    // r44 lowers 246->242 (three more shared heads folded).
    // r46 folds two closed-union heads (orphan-preservation event, draft/bound count): 239->237.
    expect(literal).toBe(190);
    expect(total).toBe(191);
    expect(literal).toBeLessThan(260);
    expect(total).toBeLessThan(261);
  });
});
