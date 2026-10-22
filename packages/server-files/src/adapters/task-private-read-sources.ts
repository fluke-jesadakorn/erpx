import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {AttachmentComponentKind} from '@erpx/server-files/public/attachments/types';

/** A routing hint never authorizes a byte read. The task owner reloads this
 * exact link/set/source tuple inside its fresh, preleased transaction. */
export type TaskPrivateAttachmentSource={targetId:string;purposeKey:string;componentKind:AttachmentComponentKind;witness:string};
export async function taskPrivateAttachmentSource(database:SqlDatabase,companyId:string,linkId:string):Promise<TaskPrivateAttachmentSource|null>{
 return database.prepare(`SELECT s.target_id "targetId",s.purpose_key "purposeKey",s.component_kind "componentKind",
 jsonb_build_object('link',to_jsonb(l),'set',to_jsonb(s),'source',to_jsonb(a))::text witness
 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
 LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
 WHERE l.id=? AND s.company_id=? AND s.target_type='staff_task'`).bind(linkId,companyId).first<TaskPrivateAttachmentSource>();
}
export function taskPrivateAttachmentSourceReadLocks(database:SqlDatabase){
 return database.prepare('LOCK TABLE attachment_links,attachment_sets,attachments IN SHARE MODE');
}
