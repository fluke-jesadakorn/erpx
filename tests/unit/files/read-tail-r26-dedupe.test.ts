import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  preservedObjectExistsSql,
  imageSnapshotSql,
  orphanReconciliationSql,
  attachmentsByIdCompanySql,
} from '../../../packages/server-files/src/adapters/read-tail-r26-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;

describe('Files read-tail r26 source/snapshot families', () => {
  test('preservedObjectExistsSql reproduces all three original literals byte for byte', () => {
    expect(preservedObjectExistsSql('attachment_storage_ledger', 'storage', 'p.source_id'))
      .toBe('EXISTS(SELECT 1 FROM attachment_storage_ledger storage WHERE storage.attachment_id=p.source_id AND storage.object_key=?)');
    expect(preservedObjectExistsSql('attachment_storage_ledger', 'storage', 'source.id'))
      .toBe('EXISTS(SELECT 1 FROM attachment_storage_ledger storage WHERE storage.attachment_id=source.id AND storage.object_key=?)');
    expect(preservedObjectExistsSql('attachment_derivatives', 'd', 'p.source_id'))
      .toBe('EXISTS(SELECT 1 FROM attachment_derivatives d WHERE d.attachment_id=p.source_id AND d.object_key=?)');
  });

  test('imageSnapshotSql reproduces both to_jsonb image reads byte for byte', () => {
    expect(imageSnapshotSql('publication', 'attachment_publications'))
      .toBe('SELECT to_jsonb(publication)::text image FROM attachment_publications publication WHERE id=?');
    expect(imageSnapshotSql('event', 'attachment_access_events'))
      .toBe('SELECT to_jsonb(event)::text image FROM attachment_access_events event WHERE id=?');
  });

  test('orphanReconciliationSql reproduces both LEFT JOIN reconciliation reads byte for byte', () => {
    expect(orphanReconciliationSql('attachment_storage_ledger', 'ledger'))
      .toBe("SELECT ledger.id FROM attachment_storage_ledger ledger\n    LEFT JOIN attachments source ON source.id=ledger.attachment_id\n    WHERE source.id IS NULL OR (source.company_id IS NOT NULL AND ledger.company_id IS DISTINCT FROM source.company_id) ORDER BY ledger.id");
    expect(orphanReconciliationSql('attachment_derivatives', 'derivative'))
      .toBe("SELECT derivative.id FROM attachment_derivatives derivative\n    LEFT JOIN attachments source ON source.id=derivative.attachment_id\n    WHERE source.id IS NULL OR (source.company_id IS NOT NULL AND derivative.company_id IS DISTINCT FROM source.company_id) ORDER BY derivative.id");
  });

  test('attachmentsByIdCompanySql reproduces both id+company projections byte for byte', () => {
    expect(attachmentsByIdCompanySql('object_key,checksum_sha256'))
      .toBe('SELECT object_key,checksum_sha256 FROM attachments WHERE id=? AND company_id=?');
    expect(attachmentsByIdCompanySql('checksum_sha256,source_state'))
      .toBe('SELECT checksum_sha256,source_state FROM attachments WHERE id=? AND company_id=?');
  });

  test('helpers are defined once and every call site composes them', () => {
    const helper = src('read-tail-r26-source-sql.ts');
    for (const name of ['preservedObjectExistsSql', 'imageSnapshotSql', 'orphanReconciliationSql', 'attachmentsByIdCompanySql']) {
      expect(helper.split(`export const ${name} =`).length - 1).toBe(1);
      expect(helper.match(/SELECT /g)).toHaveLength(4);
    }
    const rt = src('runtime-preservation-sources.ts');
    expect(rt.match(/preservedObjectExistsSql\(/g) ?? []).toHaveLength(2);
    expect(rt).not.toContain('EXISTS(SELECT 1 FROM attachment_storage_ledger storage WHERE storage.attachment_id=');
    const minio = src('runtime-minio-preservation-sources.ts');
    expect(minio.match(/preservedObjectExistsSql\(/g) ?? []).toHaveLength(1);
    expect(minio).not.toContain('EXISTS(SELECT 1 FROM attachment_derivatives d WHERE');
    expect(src('publication-content-read.ts')).toContain("imageSnapshotSql('publication','attachment_publications')");
    expect(src('support-ticket-portal-files.ts')).toContain("imageSnapshotSql('event','attachment_access_events')");
    const mig = src('attachments/migration.ts');
    expect(mig.match(/orphanReconciliationSql\(/g) ?? []).toHaveLength(2);
    expect(mig).not.toContain('SELECT ledger.id FROM attachment_storage_ledger ledger');
    expect(mig).not.toContain('SELECT derivative.id FROM attachment_derivatives derivative');
    expect(src('collaboration-message-sources.ts')).toContain("attachmentsByIdCompanySql('object_key,checksum_sha256')");
    expect(src('attachments/service-repository.ts')).toContain("attachmentsByIdCompanySql('checksum_sha256,source_state')");
  });

  test('locks, advisory statements, native writes and r10-r25 helpers stay untouched', () => {
    expect(src('publication-content-read.ts'))
      .toContain("await database.prepare('LOCK TABLE attachment_publications IN SHARE ROW EXCLUSIVE MODE').run()");
    expect(src('social-upload-cleanup.ts'))
      .toContain("db.prepare('LOCK TABLE attachments,upload_intents,attachment_links,attachment_sets,attachment_events,attachment_publications,audit_events IN SHARE ROW EXCLUSIVE MODE')");
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('SELECT count(DISTINCT candidate.id)');
    const mig = src('attachments/migration.ts');
    for (const kind of ['MIG_001', 'MIG_012', 'MIG_023']) expect(mig).toContain(`nativeWrite('${kind}'`);
    tail('read-tail-dedupe-sql.ts', [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'scansAggSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
      'LINK_ATTACHMENT_SELECT_SQL', 'LINK_SET_WHERE_EXISTS_SQL', 'PACKAGE_OBLIGATION_SCOPE_SQL',
    ]);
    tail('commerce-delivery-dedupe-sql.ts', ['COMMERCE_DELIVERY_GUARD_SQL']);
    tail('social-upload-cleanup-rows-sql.ts', ['jsonbRowsSql']);
    tail('social-draft-image-guard-sql.ts', ['socialDraftImageExistsSql']);
  });

  test('retirement:ts-statements stays strictly below the r26 BEFORE=290 baseline', () => {
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
    expect(literal).toBeLessThan(290);
    expect(total).toBeLessThan(291);
  });
});

function tail(file: string, exports: string[]) {
  const text = src(file);
  for (const name of exports) expect(text).toContain(`export const ${name} =`);
}
