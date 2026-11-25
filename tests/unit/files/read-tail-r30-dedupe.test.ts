import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { reconciliationReadySql } from '../../../packages/server-files/src/adapters/read-tail-r30-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;

/** Verbatim HEAD literals for the two call sites replaced by this family. */
const HEAD = {
  app: "SELECT ready FROM attachment_reconciliation_runs WHERE mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC LIMIT 1",
  commercial: 'SELECT ready FROM attachment_reconciliation_runs ORDER BY created_at DESC LIMIT 1',
} as const;

describe('Files read-tail r30 parameterized readiness SELECT-head family', () => {
  test('helper renders byte-identically to each original HEAD literal', () => {
    expect(reconciliationReadySql("mode='final' AND upload_frozen=true AND checksums_verified=true")).toBe(HEAD.app);
    expect(reconciliationReadySql('')).toBe(HEAD.commercial);
    expect(reconciliationReadySql('')).not.toMatch(
      /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i,
    );
  });

  test('helper is defined once; the live call site composes it and the retired one delegates to the native client', () => {
    expect(src('read-tail-r30-source-sql.ts').split('export const reconciliationReadySql =').length - 1).toBe(1);
    const app = src('app-attachment-sources.ts');
    expect(app.match(/reconciliationReadySql\(/g) ?? []).toHaveLength(1);
    expect(app).not.toContain(HEAD.app);

    // Lane 21 cut the commercial call site over to the native Files client.
    // It no longer composes the local SQL helper and carries no inline SQL.
    const com = src('commercial-usage-facts.ts');
    expect(com).not.toMatch(/SELECT /i);
    expect(com).not.toContain(HEAD.commercial);
    expect(com).not.toContain('reconciliationReadySql(');
    expect(com).toContain('readFilesCommercialUsage');
    expect(com).toContain('./native-lane-21/client');
  });

  test('locks, advisory statements, native writes and r1-r29 helpers stay untouched', () => {
    expect(src('attachments/service-repository.ts')).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(src('attachments/service-repository.ts')).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    const tail = src('read-tail-dedupe-sql.ts');
    for (const name of [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'scansAggSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
      'LINK_ATTACHMENT_SELECT_SQL', 'LINK_SET_WHERE_EXISTS_SQL', 'PACKAGE_OBLIGATION_SCOPE_SQL',
    ]) expect(tail).toContain(`export const ${name} =`);
    for (const [file, name] of [
      ['read-tail-r26-source-sql.ts', 'preservedObjectExistsSql'],
      ['read-tail-r26-source-sql.ts', 'imageSnapshotSql'],
      ['read-tail-r26-source-sql.ts', 'orphanReconciliationSql'],
      ['read-tail-r26-source-sql.ts', 'attachmentsByIdCompanySql'],
      ['read-tail-r27-source-sql.ts', 'SELECT_ATTACHMENT_ID_SQL'],
      ['read-tail-r28-source-sql.ts', 'PRIVATE_IMAGE_COLUMNS_SQL'],
    ] as const) expect(src(file)).toContain(`export const ${name} =`);
    const r29 = src('read-tail-r29-source-sql.ts');
    for (const name of [
      'PUBLICATION_ID_PREFIX_SQL', 'STORAGE_LEDGER_PRESENCE_SQL', 'LINK_SET_JOIN_SQL',
      'PUBLICATION_EXISTS_SQL', 'packageFileLinkJoinSql',
    ]) expect(r29).toContain(`export const ${name} =`);
  });

  test('retirement:ts-statements is strictly below the r30 BEFORE=269 baseline', () => {
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
    expect(literal).toBeLessThan(269);
    expect(total).toBeLessThan(270);
  });
});
