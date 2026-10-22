import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export type AttachmentPurposeFact = { id: string; purpose_key: string; component_kind: string; component_ref: string | null };
/** Read purpose descriptors through the caller's already scoped transaction. */
export async function attachmentPurposeFacts(db: SqlDatabase, companyId: string, ids: string[]): Promise<AttachmentPurposeFact[]> {
  if (!ids.length) return [];
  return (await db.prepare(`SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${ids.map(() => "?").join(",")})`).bind(companyId, ...ids).all<AttachmentPurposeFact>()).results;
}
