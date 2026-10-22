import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export type PmsDocumentRow = { property_id: string; guest_account_id: string; kind: string; snapshot_json: string; created_at: number; finance_id: string | null };

/** Document reads retain their order around the existing loadPms authorization. */
export function pmsDocumentQueries(db: SqlDatabase) {
  return {
    document: (id: string) => db.prepare("SELECT * FROM pms_documents WHERE id=?").bind(id).first<PmsDocumentRow>(),
    relatedPostings: async (propertyId: string, documentId: string) => (await db.prepare(`SELECT DISTINCT pp.journal_id FROM pms_postings pp JOIN pms_records r ON r.id=pp.record_id AND r.property_id=pp.property_id
    WHERE pp.property_id=? AND r.kind='entry' AND r.data_json::jsonb->>'guestAccountId'=(SELECT guest_account_id FROM pms_documents WHERE id=?)`).bind(propertyId, documentId).all<{ journal_id: string }>()).results,
  };
}
