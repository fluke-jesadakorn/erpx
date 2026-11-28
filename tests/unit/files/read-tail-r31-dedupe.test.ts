import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  attachmentPublicationByIdSql,
  ledgerBytesSumSql,
} from '../../../modules/files/server/src/adapters/read-tail-r31-source-sql';

const srcRoot = resolve(__dirname, '../../../modules/files/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;

/** Verbatim HEAD literals for the three call sites replaced by these families. */
const HEAD = {
  publicationById: 'SELECT * FROM attachment_publications WHERE id=?',
  publicationByIdCompany: 'SELECT * FROM attachment_publications WHERE id=? AND company_id=?',
  ledgerStored: "SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE state='stored'",
} as const;

describe('Files read-tail r31 parameterized families', () => {
  test('each helper renders byte-identically to its original HEAD literal', () => {
    expect(attachmentPublicationByIdSql(false)).toBe(HEAD.publicationById);
    expect(attachmentPublicationByIdSql(true)).toBe(HEAD.publicationByIdCompany);
    expect(ledgerBytesSumSql("state='stored'")).toBe(HEAD.ledgerStored);
    for (const sql of [HEAD.publicationById, HEAD.publicationByIdCompany, HEAD.ledgerStored])
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('helpers are defined once and every named call site composes one', () => {
    const helper = src('read-tail-r31-source-sql.ts');
    expect(helper.split('export const attachmentPublicationByIdSql =').length - 1).toBe(1);
    expect(helper.split('export const ledgerBytesSumSql =').length - 1).toBe(1);
    const sr = src('attachments/service-repository.ts');
    expect(sr.match(/attachmentPublicationByIdSql\(/g) ?? []).toHaveLength(2);
    expect(sr).not.toContain(HEAD.publicationById);
    expect(sr).not.toContain(HEAD.publicationByIdCompany);
    const mig = src('attachments/migration.ts');
    expect(mig.match(/ledgerBytesSumSql\(/g) ?? []).toHaveLength(1);
    expect(mig).not.toContain(HEAD.ledgerStored);
  });

  test('locks, advisory statements, r6 usedStorageSql and r1-r30 helpers stay untouched', () => {
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('export const usedStorageSql = "SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE company_id=? AND state IN (\'reserved\',\'stored\')"');
    expect(sr).toContain('${publicationByIdSql} FOR UPDATE');
    expect(sr).toContain('SELECT count(DISTINCT candidate.id)');
    expect(sr).toContain('SELECT COALESCE(max(revision),0)+1 AS next_revision');
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
    ] as const) for (const name of names) expect(src(file)).toContain(`export const ${name} =`);
  });

  test('retirement:ts-statements is strictly below the r31 BEFORE=268 baseline', () => {
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
    expect(literal).toBeLessThan(268);
    expect(total).toBeLessThan(269);
  });
});
