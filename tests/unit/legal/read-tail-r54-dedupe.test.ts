import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  legalItemKeyHeadSql,
  legalOrderedKeyHeadSql,
} from '../../../modules/legal/server/src/adapters/legal-read-tail-r54-sources';
import { legalPointByKeySql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r36-sources';
import { legalPointByIdAndKeySql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r34-sources';
import { legalStatePointSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r33-sources';

const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r54 byte oracles captured from the removed inline holders. */
const nextDocumentVersionSql = 'SELECT coalesce(max(version),0)+1 n FROM legal_documents WHERE item_id=?';
const routeSnapshotSql = 'SELECT * FROM legal_approval_routes WHERE item_id=? AND instance_id=?';
const commentsSql = "SELECT body,created_at,created_by FROM legal_comments WHERE item_id=? AND document_id=? AND visibility='external' ORDER BY created_at LIMIT 100";
const billingSql = 'SELECT id,title,state,data,version FROM legal_counsel_entries WHERE item_id=? AND created_by=? ORDER BY updated_at DESC LIMIT 100';
const authorizationSql = 'SELECT * FROM legal_retention_execution_authorizations WHERE destruction_id=? ORDER BY item_version DESC,created_at DESC,command_id DESC LIMIT 1';
const signersSql = 'SELECT user_id,ordinal,state,signed_at FROM legal_signers WHERE envelope_id=? ORDER BY ordinal';
const latestAuthorSql = "SELECT actor_id FROM legal_events WHERE item_id=? AND action IN ('created','update','document','document.upload','ai.accepted') ORDER BY version DESC LIMIT 1";
const lawRevisionGuardSql = "EXISTS(SELECT 1 FROM legal_law_revisions WHERE id=? AND rule_id=? AND state='approved')";
const inspectionSql = "SELECT result_json FROM legal_document_inspections WHERE id=? AND document_id=? AND kind='ocr'";
const proposedAiSql = "SELECT source_version,source_hash,proposal FROM legal_ai_proposals WHERE id=? AND item_id=? AND created_by=? AND state='proposed'";

describe('legal read-tail r54 item-key, ordered-key and state/kind tail dedupe', () => {
  it('renders byte-identical SQL for the four item-key head sites', () => {
    expect(legalItemKeyHeadSql('coalesce(max(version),0)+1 n', 'legal_documents')).toBe(nextDocumentVersionSql);
    expect(legalItemKeyHeadSql('*', 'legal_approval_routes', 'instance_id')).toBe(routeSnapshotSql);
    expect(legalItemKeyHeadSql('body,created_at,created_by', 'legal_comments', 'document_id') + " AND visibility='external' ORDER BY created_at LIMIT 100").toBe(commentsSql);
    expect(legalItemKeyHeadSql('id,title,state,data,version', 'legal_counsel_entries', 'created_by') + ' ORDER BY updated_at DESC LIMIT 100').toBe(billingSql);
    // Independent base oracle: the widened r36 head plus a trusted second key.
    expect(legalPointByKeySql('coalesce(max(version),0)+1 n', 'legal_documents', 'item_id')).toBe(nextDocumentVersionSql);
    expect(legalPointByKeySql('*', 'legal_approval_routes', 'item_id') + ' AND instance_id=?').toBe(routeSnapshotSql);
    expect(legalPointByKeySql('body,created_at,created_by', 'legal_comments', 'item_id') + ' AND document_id=?').toBe(legalItemKeyHeadSql('body,created_at,created_by', 'legal_comments', 'document_id'));
  });

  it('renders byte-identical SQL for the three ordered non-item-key head sites', () => {
    expect(legalOrderedKeyHeadSql('*', 'legal_retention_execution_authorizations', 'destruction_id') + ' ORDER BY item_version DESC,created_at DESC,command_id DESC LIMIT 1').toBe(authorizationSql);
    expect(legalOrderedKeyHeadSql('user_id,ordinal,state,signed_at', 'legal_signers', 'envelope_id') + ' ORDER BY ordinal').toBe(signersSql);
    expect(legalOrderedKeyHeadSql('actor_id', 'legal_events', 'item_id') + " AND action IN ('created','update','document','document.upload','ai.accepted') ORDER BY version DESC LIMIT 1").toBe(latestAuthorSql);
    // Independent base oracle: r36 with the widened key union.
    expect(legalPointByKeySql('*', 'legal_retention_execution_authorizations', 'destruction_id')).toBe(legalOrderedKeyHeadSql('*', 'legal_retention_execution_authorizations', 'destruction_id'));
    expect(legalPointByKeySql('user_id,ordinal,state,signed_at', 'legal_signers', 'envelope_id')).toBe(legalOrderedKeyHeadSql('user_id,ordinal,state,signed_at', 'legal_signers', 'envelope_id'));
  });

  it('renders byte-identical SQL for the three state/kind tail sites', () => {
    expect(legalStatePointSql('1', 'legal_law_revisions', 'rule_id', 'approved')).toBe("SELECT 1 FROM legal_law_revisions WHERE id=? AND rule_id=? AND state='approved'");
    expect(`EXISTS(${legalStatePointSql('1', 'legal_law_revisions', 'rule_id', 'approved')})`).toBe(lawRevisionGuardSql);
    expect(legalPointByIdAndKeySql('result_json', 'legal_document_inspections', 'document_id') + " AND kind='ocr'").toBe(inspectionSql);
    expect(legalPointByIdAndKeySql('source_version,source_hash,proposal', 'legal_ai_proposals', 'item_id') + " AND created_by=? AND state='proposed'").toBe(proposedAiSql);
    // Independent base oracle: r34 head plus the fixed kind/state tail.
    expect(`SELECT result_json FROM legal_document_inspections WHERE id=? AND document_id=?`).toBe(legalPointByIdAndKeySql('result_json', 'legal_document_inspections', 'document_id'));
  });

  it('keeps the original bind counts and no write or lock text in every rendered statement', () => {
    const binds: Array<[string, number]> = [
      [nextDocumentVersionSql, 1], [routeSnapshotSql, 2], [commentsSql, 2], [billingSql, 2],
      [authorizationSql, 1], [signersSql, 1], [latestAuthorSql, 1],
      [lawRevisionGuardSql, 2], [inspectionSql, 2], [proposedAiSql, 3],
    ];
    for (const [sql, n] of binds) {
      expect((sql.match(/\?/g) ?? []).length).toBe(n);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('adds one zero-token source with two delegating helpers and widened base key unions', async () => {
    const [r54, r33, r34, r36] = await Promise.all([
      src('legal-read-tail-r54-sources.ts'), src('legal-read-tail-r33-sources.ts'),
      src('legal-read-tail-r34-sources.ts'), src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r54).not.toContain('SELECT ');
    expect(r54.match(/export const legalItemKeyHeadSql/g) ?? []).toHaveLength(1);
    expect(r54.match(/export const legalOrderedKeyHeadSql/g) ?? []).toHaveLength(1);
    expect(r54).toContain("from './legal-read-tail-r36-sources'");
    expect(r33).toContain("'company_id' | 'document_id' | 'rule_id'");
    expect(r34).toContain("'user_id' | 'company_id' | 'item_id' | 'document_id'");
    expect(r36).toContain("'document_id' | 'id' | 'item_id' | 'destruction_id' | 'envelope_id'");
  });

  it('removes every inline holder and keeps advisory, FOR UPDATE OF and LOCK TABLE text intact', async () => {
    const [documents, approval, portal, retention, service, corpus, operations, r9, worker] = await Promise.all([
      src('legal/documents.ts'), src('legal/approval.ts'), src('legal/portal.ts'),
      src('retention-execution.ts'), src('legal/service.ts'), src('legal/law-corpus.ts'),
      src('legal/operations.ts'), src('legal-read-tail-r9-sources.ts'), src('legal/destruction-worker.ts'),
    ]);
    for (const [file, oracle] of [
      [documents, nextDocumentVersionSql], [approval, routeSnapshotSql], [approval, latestAuthorSql],
      [portal, commentsSql], [portal, billingSql], [retention, authorizationSql], [service, signersSql],
      [corpus, lawRevisionGuardSql], [corpus, inspectionSql], [operations, proposedAiSql],
    ] as Array<[string, string]>) expect(file).not.toContain(oracle);

    expect(documents).toContain('export const nextDocumentVersionSql = legalItemKeyHeadSql("coalesce(max(version),0)+1 n","legal_documents")');
    expect(approval).toContain('legalItemKeyHeadSql("*","legal_approval_routes","instance_id")');
    expect(approval).toContain('legalOrderedKeyHeadSql("actor_id","legal_events","item_id")');
    expect(portal).toContain('legalItemKeyHeadSql("body,created_at,created_by","legal_comments","document_id")');
    expect(portal).toContain('legalItemKeyHeadSql("id,title,state,data,version","legal_counsel_entries","created_by")');
    expect(retention).toContain("legalOrderedKeyHeadSql('*','legal_retention_execution_authorizations','destruction_id')");
    expect(service).toContain('legalOrderedKeyHeadSql("user_id,ordinal,state,signed_at","legal_signers","envelope_id")');
    expect(corpus).toContain("`EXISTS(${legalStatePointSql('1','legal_law_revisions','rule_id','approved')})`");
    expect(corpus).toContain("legalPointByIdAndKeySql('result_json','legal_document_inspections','document_id')");
    expect(operations).toContain('legalPointByIdAndKeySql("source_version,source_hash,proposal","legal_ai_proposals","item_id")');

    // Protected text is never routed through a helper.
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(retention).toContain('LOCK TABLE legal_settings,legal_retention_execution_authorizations');
  });

  it('retirement:ts-statements is BEFORE=173 -> AFTER=153', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(173);
    expect(total).toBeLessThanOrEqual(153);
  });
});
