import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { FIELD_LINK_EXISTS_SQL } from '../../../packages/server-files/src/adapters/read-tail-r33-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;

const HEAD_LINK = {
  finance: 'SELECT EXISTS(SELECT 1 FROM attachment_links l LEFT JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id LEFT JOIN attachments a ON a.id=l.attachment_id WHERE l.id=? ',
  field: "SELECT EXISTS(SELECT 1 FROM attachment_links l LEFT JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id WHERE l.id=? AND (s.target_type='field_service_visit'",
} as const;

describe('Files read-tail r33 parameterized family', () => {
  test('the shared head is a byte-identical prefix of both original HEAD link witnesses', () => {
    expect(FIELD_LINK_EXISTS_SQL.length).toBe(204);
    expect(HEAD_LINK.finance.startsWith(FIELD_LINK_EXISTS_SQL)).toBe(true);
    expect(HEAD_LINK.field.startsWith(FIELD_LINK_EXISTS_SQL)).toBe(true);
    expect(FIELD_LINK_EXISTS_SQL).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('both call sites compose the head and no longer inline it', () => {
    const owner = src('field-file-writer-owner.ts');
    expect((owner.match(/FIELD_LINK_EXISTS_SQL/g) ?? []).length).toBe(3); // import + 2 call sites
    expect(owner).not.toContain(HEAD_LINK.finance);
    expect(owner).not.toContain(HEAD_LINK.field);
    const helper = src('read-tail-r33-source-sql.ts');
    expect(helper.split('export const FIELD_LINK_EXISTS_SQL =').length - 1).toBe(1);
  });

  test('advisory locks, native writes and r1-r32 helpers stay untouched', () => {
    const head = (p: string) => execFileSync('git', ['show', `HEAD:${p}`], { encoding: 'utf8' });
    const sr = 'packages/server-files/src/adapters/attachments/service-repository.ts';
    expect(head(sr)).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(head(sr)).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(src('attachments/service-repository.ts')).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(src('attachments/service-repository.ts')).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    for (const h of [
      'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r27-source-sql.ts',
      'read-tail-r28-source-sql.ts', 'read-tail-r29-source-sql.ts', 'read-tail-r30-source-sql.ts',
      'read-tail-r31-source-sql.ts', 'read-tail-r32-source-sql.ts',
    ]) {
      const p = `packages/server-files/src/adapters/${h}`;
      expect(execFileSync('git', ['rev-parse', `HEAD:${p}`], { encoding: 'utf8' }).trim())
        .toBe(execFileSync('git', ['hash-object', p], { encoding: 'utf8' }).trim());
    }
  });

  test('retirement:ts-statements is strictly below the r33 BEFORE=266 baseline', () => {
    let literal = 0;
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          literal += literalSelectCount(text);
          total += selectCount(text);
        }
      }
    };
    walk(srcRoot);
    // r34 lowers the tail (264 -> 262), r35 lowers it (262 -> 260), r36
    // lowers it again (260 -> 258), r38 lowers 257 -> 255, r40 lowers 255 -> 254 and r41 lowers 254 -> 251:
    // two attestation witnesses now delegate to the r40 head helper; r41 folds three shared read heads.
    // r43 left 246; r44 folds three more shared heads to reach 242.
    // r46 folds two closed-union heads (orphan-preservation event, draft/bound count): 239->237.
    expect(literal).toBe(195);
    expect(total).toBe(196);
    expect(literal).toBeLessThan(266);
    expect(total).toBeLessThan(267);
  });
});
