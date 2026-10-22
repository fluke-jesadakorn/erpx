import "server-only";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";

type MediaActor = { userId: string };
type BrandMediaUpload = { intent: { id: string; owner_type: string; owner_id: string; purpose: string; revision: number; original_name: string }; attachmentId: string; previousId: string | null | undefined; finalKey: string; checksum: string; bytes: number; now: number };
/** The File owner stages statements; the caller commits them with its own record and receipt. */
export function prepareBrandMediaUpload(db: SqlDatabase, actor: MediaActor, input: BrandMediaUpload): SqlStatement[] {
  const { intent, attachmentId, finalKey, checksum, bytes, now } = input;
  return [
    db.prepare("UPDATE upload_intents SET status='ready',actual_bytes=?,checksum_sha256=?,final_key=?,updated_at=? WHERE id=? AND status='pending'").bind(bytes, checksum, finalKey, now, intent.id),
    db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(attachmentId, intent.id, intent.owner_type, intent.owner_id, intent.purpose, finalKey, intent.revision, intent.original_name.replace(/\.[^.]+$/, "") + ".webp", "image/webp", bytes, checksum, actor.userId, now),
  ];
}
export function prepareBrandMediaRemoval(db: SqlDatabase, actor: MediaActor, input: { attachmentId?: string | null; publicationId?: string | null; commandId: string; now: number; scope: "storefront" | "workspace" }): SqlStatement[] {
  const { attachmentId, publicationId, commandId, now, scope } = input;
  return [
    ...(publicationId ? [
      db.prepare("UPDATE attachment_publications SET state='revoked',revoked_by=?,revoked_at=?,revocation_reason=? WHERE id=? AND state='approved'").bind(actor.userId, now, scope === "storefront" ? "Removed from the active storefront brand" : "Removed from the active workspace brand", publicationId),
      db.prepare("INSERT INTO attachment_events(id,company_id,link_id,actor_id,action,details_json,created_at) SELECT ?,company_id,source_link_id,?,'publication.revoked',?,? FROM attachment_publications WHERE id=? AND state='revoked' ON CONFLICT(id) DO NOTHING").bind(`${commandId}-publication-revoke`, actor.userId, JSON.stringify({ publicationId, reason: scope === "storefront" ? "Removed from active storefront brand" : "Removed from active workspace brand" }), now, publicationId),
    ] : []),
    ...(attachmentId ? [db.prepare("UPDATE attachments SET revoked_at=? WHERE id=? AND revoked_at IS NULL").bind(now, attachmentId)] : []),
  ];
}
export async function brandMediaObjectKey(db: SqlDatabase, attachmentId: string): Promise<string | null> {
  return (await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(attachmentId).first<{ object_key: string }>())?.object_key ?? null;
}
