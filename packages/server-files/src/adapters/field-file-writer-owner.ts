import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {holdingResources} from '@erpx/server-runtime/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
const fieldSetMarker="(s.target_type='field_service_visit' OR EXISTS(SELECT 1 FROM attachment_links l LEFT JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id WHERE l.set_id=s.id AND (u.owner_type='field_service_visit' OR us.target_type='field_service_visit' OR erpx_files_field_attachment_owned(l.attachment_id))) OR EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=s.id AND (u.owner_type='field_service_visit' OR EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND erpx_files_field_attachment_owned(a.id)))))";
export type FieldFileResource={kind:'set'|'upload'|'link'|'attachment';id:string};
/** Persisted transport selection only. This never grants file access or replaces
 * the Rust owner's fresh source/Company/permission checks inside its transaction.
 * A foreign/corrupt Field marker must never become a legacy fallback. */
export async function fieldFileWriterOwnerInDatabase(db:SqlDatabase,actor:Pick<WorkContext,'companyId'>,resource:FieldFileResource):Promise<'field'|'other'>{
 if(!resource.id||resource.id.includes("\u0000"))throw new ApiError(404,'ATTACHMENT_NOT_FOUND','File evidence not found.');
 const queries={
  set:`SELECT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=? AND ${fieldSetMarker}) marker,EXISTS(SELECT 1 FROM attachment_sets WHERE id=? AND company_id=?) scoped`,
  upload:"SELECT EXISTS(SELECT 1 FROM upload_intents u LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id WHERE u.id=? AND (u.owner_type='field_service_visit' OR s.target_type='field_service_visit' OR EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND erpx_files_field_attachment_owned(a.id)))) OR EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.upload_intent_id=? AND s.target_type='field_service_visit') marker,EXISTS(SELECT 1 FROM upload_intents WHERE id=? AND company_id=?) scoped",
  link:"SELECT EXISTS(SELECT 1 FROM attachment_links l LEFT JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id WHERE l.id=? AND (s.target_type='field_service_visit' OR u.owner_type='field_service_visit' OR us.target_type='field_service_visit' OR erpx_files_field_attachment_owned(l.attachment_id))) marker,EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=?) scoped",
  attachment:"SELECT erpx_files_field_attachment_owned(?) marker,EXISTS(SELECT 1 FROM attachments WHERE id=? AND company_id=?) scoped",
 } as const;
 const values=resource.kind==='upload'?[resource.id,resource.id,resource.id,actor.companyId]:[resource.id,resource.id,actor.companyId];
 const row=await db.prepare(queries[resource.kind]).bind(...values).first<{marker:boolean;scoped:boolean}>();
 if(!row||typeof row.marker!=='boolean'||typeof row.scoped!=='boolean')throw new ApiError(503,'ATTACHMENT_OWNER_UNAVAILABLE','File evidence ownership is unavailable.');
 if(!row.marker)return 'other';
 if(!row.scoped)throw new ApiError(404,'ATTACHMENT_NOT_FOUND','File evidence not found.');
 return 'field';
}
export async function fieldFileWriterOwner(actor:WorkContext,resource:FieldFileResource){return fieldFileWriterOwnerInDatabase((await holdingResources(actor.holdingId)).write,actor,resource);}

/** Fail-closed metadata preflight for shared target-list callers. */
export async function fieldTargetHasNativeFilesInDatabase(db:SqlDatabase,actor:Pick<WorkContext,'companyId'>,target:{targetType:string;targetId:string}):Promise<boolean>{
 if(target.targetType==='field_service_visit')return true;
 const row=await db.prepare(`SELECT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.company_id=? AND s.target_type=? AND s.target_id=? AND ${fieldSetMarker}) marker`).bind(actor.companyId,target.targetType,target.targetId).first<{marker:boolean}>();
 if(!row||typeof row.marker!=='boolean')throw new ApiError(503,'ATTACHMENT_OWNER_UNAVAILABLE','File evidence ownership is unavailable.');
 return row.marker;
}
