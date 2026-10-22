import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase,StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {attachmentReadSource,assertAttachmentReadRoute,attachmentReadChanged,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '@erpx/server-files/public/private-attachment-reads';
import {legalRequestAttachmentSourceLocks,legalRequestAttachmentSourceImage} from '@erpx/server-records/public/legal-attachment-read-sources';
import {legalPeopleAttachmentSourceLocks,legalPeopleAttachmentSourceImage} from '@erpx/server-people_payroll/public/legal-attachment-read-sources';
import {legalInvoiceAttachmentSourceLocks,legalInvoiceAttachmentSourceImage} from '@erpx/server-finance/public/legal-attachment-read-sources';
import {requireLegalAttachmentReadPreparation,deferLegalAttachmentReadCheck} from '@erpx/server-legal/public/attachment-read-parent';

export type LegalEvidenceAttachmentType='employee_request'|'expense_claim'|'leave_request'|'expense_payment'|'invoice_payment';
export type LegalEvidenceSource={attachmentId:string;ownerType:LegalEvidenceAttachmentType;ownerId:string};
const identity=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.userId,actor.email,actor.assignmentId,actor.role,actor.branchId,actor.departmentId,actor.scope,actor.assignmentScope,actor.modules,actor.authority??null,actor.iam??null]);
const parents=new WeakMap<SqlDatabase,{identity:string;source:LegalEvidenceSource}>();
export function isLegalEvidenceAttachmentType(kind:string):kind is LegalEvidenceAttachmentType{return ['employee_request','expense_claim','leave_request','expense_payment','invoice_payment'].includes(kind);}
/** Presence only selects the strict reader. It must never fall through to a
 * legacy read when the selected actor differs from this registered parent. */
export function hasLegalEvidenceAttachmentReadParent(database:SqlDatabase){return parents.has(database);}
/** Classification comes from the actual immutable File Source owner. The
 * selected source is re-read under leases before any bytes are accepted. */
export async function prepareLegalEvidenceAttachmentParentRead(database:SqlDatabase,actor:WorkContext,source:LegalEvidenceSource){
 requireLegalAttachmentReadPreparation(database,actor.companyId);
 const locks=source.ownerType==='employee_request'?legalRequestAttachmentSourceLocks(database):source.ownerType==='invoice_payment'?legalInvoiceAttachmentSourceLocks(database,actor.companyId):legalPeopleAttachmentSourceLocks(database,source.ownerType);
 await database.batch(locks);parents.set(database,{identity:identity(actor),source});
}
async function image(database:SqlDatabase,actor:WorkContext,source:LegalEvidenceSource){return source.ownerType==='employee_request'?legalRequestAttachmentSourceImage(database,actor.companyId,source.ownerId):source.ownerType==='invoice_payment'?legalInvoiceAttachmentSourceImage(database,actor.companyId,source.ownerId):legalPeopleAttachmentSourceImage(database,actor.companyId,source.ownerType,source.ownerId);}
function changed(){return new ApiError(409,'LEGAL_EVIDENCE_FILE_READ_CHANGED','The original preserved evidence or selected authority changed before Legal completed its copy.');}
export async function withLegalEvidenceAttachmentContentRead<Result extends {object:StoredFile}>(database:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 const parent=parents.get(database);if(!parent||parent.identity!==identity(actor))throw new ApiError(503,'LEGAL_EVIDENCE_READ_PARENT_REQUIRED','Evidence bytes require the original complete Legal preservation transaction.');
 if(selected.targetType!==parent.source.ownerType||selected.targetId!==parent.source.ownerId)throw attachmentReadChanged();
 const original=await database.prepare('SELECT 1 FROM attachment_links l JOIN attachments a ON a.id=l.attachment_id WHERE l.id=? AND a.id=? AND a.company_id=? AND a.owner_type=? AND a.owner_id=?').bind(linkId,parent.source.attachmentId,actor.companyId,parent.source.ownerType,parent.source.ownerId).first();if(!original)throw attachmentReadChanged();
 const before=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,before);if(!before)throw attachmentReadChanged();const sourceImage=await image(database,actor,parent.source);if(!sourceImage)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original evidence owner is unavailable.');
 const result=await read(database),bytes=await result.object.arrayBuffer();
 deferLegalAttachmentReadCheck(database,actor.companyId,async()=>{
  if(parent.identity!==identity(actor))throw changed();try{await requirePrivateAttachmentReaderCurrent(database,actor);}catch(error){if(error instanceof ApiError&&error.code==='ATTACHMENT_FILE_READ_CHANGED')throw changed();throw error;}
  await authorize(database,before);const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(before,after);
  if(after?.witness!==before.witness||await image(database,actor,parent.source)!==sourceImage)throw changed();
 });
 return bufferedAttachmentRead(result,bytes);
}
