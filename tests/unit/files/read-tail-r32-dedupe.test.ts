import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { ledgerStateRowsSql } from '../../../packages/server-files/src/adapters/read-tail-r32-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;

/** Verbatim HEAD literals for the two call sites replaced by this family. */
const HEAD = {
  stored: "SELECT id,object_key,bytes,variant FROM attachment_storage_ledger WHERE attachment_id=? AND state='stored' ORDER BY id",
  destroyed: "SELECT id,object_key,bytes,variant FROM attachment_storage_ledger WHERE attachment_id=? AND state IN ('stored','destroyed') ORDER BY id",
} as const;

describe('Files read-tail r32 parameterized family', () => {
  test('the helper renders byte-identically to both original HEAD literals', () => {
    expect(ledgerStateRowsSql("state='stored'")).toBe(HEAD.stored);
    expect(ledgerStateRowsSql("state IN ('stored','destroyed')")).toBe(HEAD.destroyed);
    for (const sql of [HEAD.stored, HEAD.destroyed])
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('helper is defined once and both named call sites compose it', () => {
    const helper = src('read-tail-r32-source-sql.ts');
    expect(helper.split('export const ledgerStateRowsSql =').length - 1).toBe(1);
    const ld = src('legal-destruction-source-facts.ts');
    expect(ld.match(/ledgerStateRowsSql\(/g) ?? []).toHaveLength(2);
    expect(ld).not.toContain(HEAD.stored);
    expect(ld).not.toContain(HEAD.destroyed);
  });

  test('locks, advisory statements, native writes and r1-r31 helpers stay untouched', () => {
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('export const usedStorageSql = "SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE company_id=? AND state IN (\'reserved\',\'stored\')"');
    const mig = src('attachments/migration.ts');
    for (const kind of ['MIG_001', 'MIG_012', 'MIG_023']) expect(mig).toContain(`nativeWrite('${kind}'`);
    const tail = src('read-tail-dedupe-sql.ts');
    for (const name of [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'scansAggSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
      'LINK_ATTACHMENT_SELECT_SQL', 'LINK_SET_WHERE_EXISTS_SQL', 'PACKAGE_OBLIGATION_SCOPE_SQL',
    ]) expect(tail).toContain(`export const ${name} =`);
    for (const [file, names] of [
      ['read-tail-r26-source-sql.ts', ['preservedObjectExistsSql', 'imageSnapshotSql', 'orphanReconciliationSql', 'attachmentsByIdCompanySql']],
      ['read-tail-r27-source-sql.ts', ['SELECT_ATTACHMENT_ID_SQL']],
      ['read-tail-r28-source-sql.ts', ['PRIVATE_IMAGE_COLUMNS_SQL']],
      ['read-tail-r29-source-sql.ts', ['PUBLICATION_ID_PREFIX_SQL', 'STORAGE_LEDGER_PRESENCE_SQL', 'LINK_SET_JOIN_SQL', 'PUBLICATION_EXISTS_SQL', 'packageFileLinkJoinSql']],
      ['read-tail-r30-source-sql.ts', ['reconciliationReadySql']],
      ['read-tail-r31-source-sql.ts', ['attachmentPublicationByIdSql', 'ledgerBytesSumSql']],
    ] as const) for (const name of names) expect(src(file)).toContain(`export const ${name} =`);
  });

  test('retirement:ts-statements is strictly below the r32 BEFORE=267 baseline', () => {
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
    expect(literal).toBeLessThan(267);
    expect(total).toBeLessThan(268);
  });
});
