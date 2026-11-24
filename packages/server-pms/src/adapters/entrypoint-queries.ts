import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativePmsReadAll, nativePmsReadFirst, PMS_RD_DOCUMENT_BY_ID, PMS_RD_DOCUMENT_RELATED_POSTINGS } from "./native-pms-read";
export type PmsDocumentRow = { property_id: string; guest_account_id: string; kind: string; snapshot_json: string; created_at: number; finance_id: string | null };

/** Document reads retain their order around the existing loadPms authorization. */
export function pmsDocumentQueries(db: SqlDatabase) {
  return {
    document: (id: string) => nativePmsReadFirst<PmsDocumentRow>(PMS_RD_DOCUMENT_BY_ID, [id], db),
    relatedPostings: async (propertyId: string, documentId: string) =>
      (await nativePmsReadAll<{ journal_id: string }>(PMS_RD_DOCUMENT_RELATED_POSTINGS, [propertyId, documentId], db)).results,
  };
}
