import { describe, expect, test } from 'vitest';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { ATTACHMENT_REFERENCE_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r34-source-sql';
import { STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r41-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;
const git = (...a: string[]) => execFileSync('git', a, { encoding: 'utf8' });

/** Verbatim HEAD literals for the two call sites replaced by this family. */
const HEAD = {
  service: 'SELECT 1 WHERE EXISTS(SELECT 1 FROM attachments WHERE company_id=? AND (object_key=? OR preview_object_key=?)) OR EXISTS(${STORAGE_LEDGER_PRESENCE_SQL} AND object_key=?) OR EXISTS(SELECT 1 FROM upload_intents WHERE company_id=? AND id<>? AND ? IN (staging_key,quarantine_key,final_key))',
  messaging: 'SELECT 1 WHERE EXISTS(SELECT 1 FROM attachments WHERE object_key=? OR preview_object_key=?) OR EXISTS(SELECT 1 FROM upload_intents WHERE quarantine_key=? OR staging_key=?) OR EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE object_key=?) OR ${PUBLICATION_EXISTS_SQL} WHERE derivative_object_key=?) OR EXISTS(SELECT 1 FROM attachment_orphan_preservations WHERE object_key=?)',
} as const;

describe('Files read-tail r34 parameterized family', () => {
  test('the shared head is a byte-identical prefix of both original HEAD literals', () => {
    expect(ATTACHMENT_REFERENCE_HEAD_SQL.length).toBe(54);
    expect(HEAD.service.startsWith(ATTACHMENT_REFERENCE_HEAD_SQL)).toBe(true);
    expect(HEAD.messaging.startsWith(ATTACHMENT_REFERENCE_HEAD_SQL)).toBe(true);
    expect((ATTACHMENT_REFERENCE_HEAD_SQL.match(/SELECT /g) ?? []).length).toBe(2);
    expect(ATTACHMENT_REFERENCE_HEAD_SQL).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    // novel: no r10-r33 helper already carried this head
    for (const h of [
      'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r27-source-sql.ts',
      'read-tail-r28-source-sql.ts', 'read-tail-r29-source-sql.ts', 'read-tail-r30-source-sql.ts',
      'read-tail-r31-source-sql.ts', 'read-tail-r32-source-sql.ts', 'read-tail-r33-source-sql.ts',
    ]) expect(src(h)).not.toContain(ATTACHMENT_REFERENCE_HEAD_SQL);
  });

  test('both call sites compose the head and render byte-identically', () => {
    // Render the r44 upload_intents splice back to its pre-r44 literal bytes.
    const head44 = 'EXISTS(SELECT 1 FROM upload_intents WHERE ';
    const renderR44 = (t: string) => t
      .replaceAll('" + UPLOAD_INTENTS_EXISTS_HEAD_SQL + "', head44)
      .replaceAll('${UPLOAD_INTENTS_EXISTS_HEAD_SQL}', head44);
    const svc = renderR44(src('attachments/service.ts'));
    const msg = renderR44(src('messaging-upload-rollback.ts'));
    expect((svc.match(/ATTACHMENT_REFERENCE_HEAD_SQL/g) ?? []).length).toBe(2); // import + call site
    expect((msg.match(/ATTACHMENT_REFERENCE_HEAD_SQL/g) ?? []).length).toBe(2); // import + call site
    const svcTail = svc.match(/ATTACHMENT_REFERENCE_HEAD_SQL \+ "(company_id=\?[^"]*)"/)![1];
    const msgTail = msg.match(/\$\{ATTACHMENT_REFERENCE_HEAD_SQL\}(object_key=\?[^`]*)`/)![1];
    expect(ATTACHMENT_REFERENCE_HEAD_SQL + svcTail).toBe(HEAD.service);
    expect((ATTACHMENT_REFERENCE_HEAD_SQL + msgTail).replaceAll('${STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL}', STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL)).toBe(HEAD.messaging);
    expect(svc).not.toContain(HEAD.service);
    expect(msg).not.toContain(HEAD.messaging);
    const helper = src('read-tail-r34-source-sql.ts');
    expect(helper.split('export const ATTACHMENT_REFERENCE_HEAD_SQL =').length - 1).toBe(1);
  });

  test('advisory locks, native writes and r1-r33 helpers stay untouched', () => {
    const svcHeadSrc = git('show', 'HEAD:packages/server-files/src/adapters/attachments/service.ts');
    const msgHeadSrc = git('show', 'HEAD:packages/server-files/src/adapters/messaging-upload-rollback.ts');
    const svc = src('attachments/service.ts');
    const msg = src('messaging-upload-rollback.ts');
    for (const [before, after, needle] of [
      [svcHeadSrc, svc, 'db.prepare(advisoryLockSql).bind(key).run()'],
      [svcHeadSrc, svc, 'legacyWriterAssertSql'],
      [msgHeadSrc, msg, 'LOCK TABLE attachments,upload_intents'],
      [msgHeadSrc, msg, 'SELECT pg_advisory_xact_lock(hashtextextended(?,0))'],
      [msgHeadSrc, msg, 'store.delete(key,store.supportsTransactionDeletion'],
    ] as const) {
      expect(before).toContain(needle);
      expect(after).toContain(needle);
    }
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    for (const h of [
      'read-tail-dedupe-sql.ts', 'read-tail-r26-source-sql.ts', 'read-tail-r27-source-sql.ts',
      'read-tail-r28-source-sql.ts', 'read-tail-r29-source-sql.ts', 'read-tail-r30-source-sql.ts',
      'read-tail-r31-source-sql.ts', 'read-tail-r32-source-sql.ts', 'read-tail-r33-source-sql.ts',
    ]) {
      const p = `packages/server-files/src/adapters/${h}`;
      expect(git('rev-parse', `HEAD:${p}`).trim()).toBe(git('hash-object', p).trim());
    }
  });

  test('retirement:ts-statements is strictly below the r34 BEFORE=264 baseline', () => {
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
    // r42 left 248; r43 lowers 248->246 (two SELECT-bearing shared heads folded).
    // r44 lowers 246->242 (three more shared heads folded).
    // r46 folds two closed-union heads (orphan-preservation event, draft/bound count): 239->237.
    expect(literal).toBe(204);
    expect(total).toBe(205);
    expect(literal).toBeLessThan(264);
    expect(total).toBeLessThan(265);
  });
});
