import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { attachmentSetPurposeSql } from "./read-tail-dedupe-sql";
export type AttachmentPurposeFact = { id: string; purpose_key: string; component_kind: string; component_ref: string | null };
/** Read purpose descriptors through the caller's already scoped transaction. */
export async function attachmentPurposeFacts(db: SqlDatabase, companyId: string, ids: string[]): Promise<AttachmentPurposeFact[]> {
  if (!ids.length) return [];
  return (await db.prepare(attachmentSetPurposeSql(ids.map(() => "?").join(","))).bind(companyId, ...ids).all<AttachmentPurposeFact>()).results;
}
