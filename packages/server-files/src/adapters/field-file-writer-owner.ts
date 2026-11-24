import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {holdingResources} from '@erpx/server-runtime/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {LINK_SET_WHERE_EXISTS_SQL} from './read-tail-dedupe-sql';
import {FIELD_LINK_EXISTS_SQL} from './read-tail-r33-source-sql';
import {FIELD_UPLOAD_OWNER_HEAD_SQL} from './read-tail-r35-source-sql';
import {FIELD_SET_OWNER_HEAD_SQL} from './read-tail-r36-source-sql';
import {FIELD_ATTACHMENT_OWNED_HEAD_SQL,FIELD_SET_MARKER_EXISTS_HEAD_SQL} from './read-tail-r41-source-sql';
/** Byte-identical scoped-existence witnesses shared by the two Field owner
 * preflights. Text only: the table token comes from a fixed closed set and every
 * call site still prepares its own statement with its own binds. */
const scopedByCompanySql=(table:'attachment_sets'|'upload_intents'|'attachments')=>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=?) scoped`;
const scopedLinkSql=`${LINK_SET_WHERE_EXISTS_SQL}id=? AND s.company_id=?) scoped`;
const fieldSetMarker=`(s.target_type='field_service_visit'${FIELD_SET_MARKER_EXISTS_HEAD_SQL}WHERE l.set_id=s.id AND (u.owner_type='field_service_visit' OR us.target_type='field_service_visit' OR erpx_files_field_attachment_owned(l.attachment_id))) OR EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=s.id AND (u.owner_type='field_service_visit' OR EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND erpx_files_field_attachment_owned(a.id)))))`;
export type FieldFileResource={kind:'set'|'upload'|'link'|'attachment';id:string};
function financeMarkers(kind:'payment'|'journal'){
 const type=kind==='payment'?'finance_payment':'finance_journal';
 const owned=(id:string)=>kind==='payment'?`erpx_finance_field_payment_owned(${id})`:`erpx_finance_field_journal_owned(${id},NULL)`;
 const serviceOwner=`(u.owner_type='${type}' AND ${owned('u.owner_id')})`,serviceSet=`(s.target_type='${type}' AND ${owned('s.target_id')})`,serviceAncestor=`(us.target_type='${type}' AND ${owned('us.target_id')})`,serviceSource=`(a.owner_type='${type}' AND ${owned('a.owner_id')})`;
 const serviceSetMarker=`(${serviceSet}${FIELD_SET_MARKER_EXISTS_HEAD_SQL}LEFT JOIN attachments a ON a.id=l.attachment_id WHERE l.set_id=s.id AND (${serviceOwner} OR ${serviceAncestor} OR ${serviceSource})) OR EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=s.id AND ${serviceOwner}))`;
 return {serviceOwner,serviceSet,serviceAncestor,serviceSource,serviceSetMarker};
}
/** Finance policy supplies persisted Field ownership; it grants no file access.
 * All consuming Rust operations recheck actual current Company/record lineage. */
async function financeFileOwnerInDatabase(db:SqlDatabase,actor:Pick<WorkContext,'companyId'>,resource:FieldFileResource,kind:'payment'|'journal'):Promise<boolean>{
 const {serviceOwner,serviceSet,serviceAncestor,serviceSource,serviceSetMarker}=financeMarkers(kind);
 if(!resource.id||resource.id.includes("\u0000"))throw new ApiError(404,'ATTACHMENT_NOT_FOUND','File evidence not found.');
 const queries={
  set:`${FIELD_SET_OWNER_HEAD_SQL}${serviceSetMarker}) marker,${scopedByCompanySql('attachment_sets')}`,
  upload:`${FIELD_UPLOAD_OWNER_HEAD_SQL}${serviceOwner} OR ${serviceSet})) OR ${LINK_SET_WHERE_EXISTS_SQL}upload_intent_id=? AND ${serviceSet}) marker,${scopedByCompanySql('upload_intents')}`,
  link:`${FIELD_LINK_EXISTS_SQL}LEFT JOIN attachments a ON a.id=l.attachment_id WHERE l.id=? AND (${serviceSet} OR ${serviceOwner} OR ${serviceAncestor} OR ${serviceSource})) marker,${scopedLinkSql}`,
  attachment:`SELECT EXISTS(SELECT 1 FROM attachments a WHERE a.id=? AND (${serviceSource} OR ${LINK_SET_WHERE_EXISTS_SQL}attachment_id=a.id AND ${serviceSet}))) marker,${scopedByCompanySql('attachments')}`,
 } as const;
 const values=resource.kind==='upload'?[resource.id,resource.id,resource.id,actor.companyId]:[resource.id,resource.id,actor.companyId];
 const row=await db.prepare(queries[resource.kind]).bind(...values).first<{marker:boolean;scoped:boolean}>();
 if(!row||typeof row.marker!=='boolean'||typeof row.scoped!=='boolean')throw new ApiError(503,'ATTACHMENT_OWNER_UNAVAILABLE','File evidence ownership is unavailable.');
 if(!row.marker)return false;
 if(!row.scoped)throw new ApiError(404,'ATTACHMENT_NOT_FOUND','File evidence not found.');
 return true;
}
export async function servicePaymentFileOwnerInDatabase(db:SqlDatabase,actor:Pick<WorkContext,'companyId'>,resource:FieldFileResource){return financeFileOwnerInDatabase(db,actor,resource,'payment');}
export async function fieldJournalFileOwnerInDatabase(db:SqlDatabase,actor:Pick<WorkContext,'companyId'>,resource:FieldFileResource){return financeFileOwnerInDatabase(db,actor,resource,'journal');}
export async function fieldJournalFileOwner(actor:WorkContext,resource:FieldFileResource){return fieldJournalFileOwnerInDatabase((await holdingResources(actor.holdingId)).write,actor,resource);}
export async function fieldJournalFilesTargetInDatabase(db:SqlDatabase,targetId:string):Promise<boolean>{
 const row=await db.prepare('SELECT erpx_finance_field_journal_owned(?,NULL) marker').bind(targetId).first<{marker:boolean}>();
 if(!row||typeof row.marker!=='boolean')throw new ApiError(503,'ATTACHMENT_OWNER_UNAVAILABLE','File evidence ownership is unavailable.');
 return row.marker;
}
export async function fieldJournalFilesTarget(actor:WorkContext,targetId:string){return fieldJournalFilesTargetInDatabase((await holdingResources(actor.holdingId)).write,targetId);}
export async function servicePaymentFileOwner(actor:WorkContext,resource:FieldFileResource){return servicePaymentFileOwnerInDatabase((await holdingResources(actor.holdingId)).write,actor,resource);}
export async function servicePaymentFilesTargetInDatabase(db:SqlDatabase,targetId:string):Promise<boolean>{
 const row=await db.prepare('SELECT erpx_finance_field_payment_owned(?) marker').bind(targetId).first<{marker:boolean}>();
 if(!row||typeof row.marker!=='boolean')throw new ApiError(503,'ATTACHMENT_OWNER_UNAVAILABLE','File evidence ownership is unavailable.');
 return row.marker;
}
export async function servicePaymentFilesTarget(actor:WorkContext,targetId:string){return servicePaymentFilesTargetInDatabase((await holdingResources(actor.holdingId)).write,targetId);}

/** Persisted transport selection only. This never grants file access or replaces
 * the Rust owner's fresh source/Company/permission checks inside its transaction.
 * A foreign/corrupt Field marker must never become a legacy fallback. */
export async function fieldFileWriterOwnerInDatabase(db:SqlDatabase,actor:Pick<WorkContext,'companyId'>,resource:FieldFileResource):Promise<'field'|'other'>{
 if(await servicePaymentFileOwnerInDatabase(db,actor,resource)||await fieldJournalFileOwnerInDatabase(db,actor,resource))return 'field';
 if(!resource.id||resource.id.includes("\u0000"))throw new ApiError(404,'ATTACHMENT_NOT_FOUND','File evidence not found.');
 const queries={
  set:`${FIELD_SET_OWNER_HEAD_SQL}${fieldSetMarker}) marker,${scopedByCompanySql('attachment_sets')}`,
  upload:`${FIELD_UPLOAD_OWNER_HEAD_SQL}u.owner_type='field_service_visit' OR s.target_type='field_service_visit' OR EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND erpx_files_field_attachment_owned(a.id)))) OR ${LINK_SET_WHERE_EXISTS_SQL}upload_intent_id=? AND s.target_type='field_service_visit') marker,${scopedByCompanySql('upload_intents')}`,
  link:`${FIELD_LINK_EXISTS_SQL}WHERE l.id=? AND (s.target_type='field_service_visit' OR u.owner_type='field_service_visit' OR us.target_type='field_service_visit' OR erpx_files_field_attachment_owned(l.attachment_id))) marker,${scopedLinkSql}`,
  attachment:`${FIELD_ATTACHMENT_OWNED_HEAD_SQL} marker,${scopedByCompanySql('attachments')}`,
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
 if(target.targetType==='finance_journal'&&await fieldJournalFilesTargetInDatabase(db,target.targetId))return true;
 if(target.targetType==='finance_payment'&&await servicePaymentFilesTargetInDatabase(db,target.targetId))return true;
 const row=await db.prepare(`SELECT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.company_id=? AND s.target_type=? AND s.target_id=? AND ${fieldSetMarker}) marker`).bind(actor.companyId,target.targetType,target.targetId).first<{marker:boolean}>();
 if(!row||typeof row.marker!=='boolean')throw new ApiError(503,'ATTACHMENT_OWNER_UNAVAILABLE','File evidence ownership is unavailable.');
 return row.marker;
}
