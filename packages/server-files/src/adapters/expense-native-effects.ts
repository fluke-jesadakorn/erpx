import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Expense consumes owner effects on its supplied transaction; this port only prepares the original statements in their original order. */
export function expenseNativeEffects(db:SqlDatabase){return {
withdrawLink:(facts:{at:number;actorId:string;reason:string;linkId:string})=>db.prepare("UPDATE attachment_links SET state='withdrawn',withdrawn_at=?,withdrawn_by=?,withdrawal_reason=? WHERE id=? AND state='draft'").bind(facts.at,facts.actorId,facts.reason,facts.linkId),
withdrawEvent:(facts:{id:string;companyId:string;attachmentId:string;linkId:string;setId:string;actorId:string;details:string;at:number})=>db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?, 'link.withdrawn',?,?)").bind(facts.id,facts.companyId,facts.attachmentId,facts.linkId,facts.setId,facts.actorId,facts.details,facts.at),
revokeAttachment:(facts:{at:number;attachmentId:string})=>db.prepare("UPDATE attachments SET revoked_at=? WHERE id=?").bind(facts.at,facts.attachmentId)
};}
