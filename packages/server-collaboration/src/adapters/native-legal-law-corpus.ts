import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal law-corpus statements. */
export function legalLawCorpus(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    insertChunk: (f: {
      identifier: string; companyId: string; itemId: string; documentId: string; checksum: string;
      revisionId: string | null; reviewId: string | null; page: number; pageMapping: boolean;
      section: string; text: string; textChecksum: string;
    }) => call('legal_source_chunk_insert', f.identifier,
      [f.identifier, f.companyId, f.itemId, f.documentId, f.checksum, f.revisionId, f.reviewId, f.page, f.pageMapping, f.section, f.text, f.textChecksum]),
    registerSource: (f: { key: string; companyId: string; id: string; title: string; fact4: string; actorId: string; now: number }) =>
      call('legal_law_source_register', f.key, [f.key, f.companyId, f.id, f.title, f.fact4, f.actorId, f.now]),
    importRevision: (f: {
      key: string; companyId: string; id: string; sourceId: string; id2: string; checksum: string;
      revisionLabel: string; publishedOn: string; effectiveFrom: string; fact9: string | null;
      extractionMethod: string; fact11: string | null; actorId: string; now: number;
    }) => call('legal_law_revision_import', f.key,
      [f.key, f.companyId, f.id, f.sourceId, f.id2, f.checksum, f.revisionLabel, f.publishedOn, f.effectiveFrom, f.fact9, f.extractionMethod, f.fact11, f.actorId, f.now]),
    bindSourceCheck: (f: { key: string; sourceCheckId: string }) =>
      call('legal_source_check_bind', '', [f.key, f.sourceCheckId]),
    importDocumentText: (f: {
      key: string; companyId: string; id: string; id2: string; checksum: string;
      extractionMethod: string; fact6: string | null; actorId: string; now: number;
    }) => call('legal_text_review_import', f.key,
      [f.key, f.companyId, f.id, f.id2, f.checksum, f.extractionMethod, f.fact6, f.actorId, f.now]),
    proposeApplicability: (f: {
      key: string; companyId: string; id: string; revisionId: string; applies: boolean;
      basis: string; actorId: string; now: number;
    }) => call('legal_applicability_propose', f.key,
      [f.key, f.companyId, f.id, f.revisionId, f.applies, f.basis, f.actorId, f.now]),
    approveApplicability: (f: { actorId: string; now: number; id: string }) =>
      call('legal_applicability_approve', '', [f.actorId, f.now, f.id]),
    reviewSourceCheck: (f: { id: string }) => call('legal_source_check_review', '', [f.id]),
    advanceRecord: (f: { now: number; id: string }) => call('legal_advance_record', '', [f.now, f.id]),
    accessEvent: (f: { identifier: string; itemId: string; documentId: string; action: string; now: number }) =>
      call('legal_access_event', f.identifier, [f.identifier, f.itemId, f.documentId, userId, f.action, f.now]),
    event: (f: { id: string; action: string; note: string; version: number; identifier: string; now: number }) =>
      call('legal_approval_event', f.identifier, [f.identifier, f.id, f.action, userId, f.note, f.version, f.now]),
  };
}
