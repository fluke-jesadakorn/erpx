import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {legalRequestAttachmentSourceLocks,legalRequestAttachmentSourceImage} from '@erpx/server-records/public/legal-attachment-read-sources';
import {legalPeopleAttachmentSourceLocks,legalPeopleAttachmentSourceImage} from '@erpx/server-people_payroll/public/legal-attachment-read-sources';
import {legalInvoiceAttachmentSourceLocks,legalInvoiceAttachmentSourceImage} from '@erpx/server-finance/public/legal-attachment-read-sources';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,attachmentReadChanged,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '@erpx/server-files/adapters/private-attachment-reads';
import {attachmentDeniedReadAudits} from '@erpx/server-files/adapters/attachment-denied-read-audits';
export type EmployeeEvidenceAttachmentType='employee_request'|'expense_claim'|'leave_request'|'expense_payment'|'invoice_payment';
export function isEmployeeEvidenceAttachmentType(kind:string):kind is EmployeeEvidenceAttachmentType{return ['employee_request','expense_claim','leave_request','expense_payment','invoice_payment'].includes(kind);}
const identity=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.userId,actor.email,actor.assignmentId,actor.role,actor.branchId,actor.departmentId,actor.assignmentScope,actor.scope,actor.modules,actor.authority??null,actor.iam??null]);
function leases(database:SqlDatabase,companyId:string,kind:EmployeeEvidenceAttachmentType){return kind==='employee_request'?legalRequestAttachmentSourceLocks(database):kind==='invoice_payment'?legalInvoiceAttachmentSourceLocks(database,companyId):legalPeopleAttachmentSourceLocks(database,kind);}
function image(database:SqlDatabase,companyId:string,kind:EmployeeEvidenceAttachmentType,id:string){return kind==='employee_request'?legalRequestAttachmentSourceImage(database,companyId,id):kind==='invoice_payment'?legalInvoiceAttachmentSourceImage(database,companyId,id):legalPeopleAttachmentSourceImage(database,companyId,kind,id);}
/** Ordinary self-service and payable evidence keep their original full File
 * owner policy. The lease ports contain native sources only, with no Legal,
 * People or Finance purchase/role inferred from this read constructor. */
export async function withEmployeeEvidenceAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(!isEmployeeEvidenceAttachmentType(selected.targetType))throw attachmentReadChanged();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'EMPLOYEE_EVIDENCE_READ_TRANSACTION_REQUIRED','Private business evidence requires its original fresh source-leased read transaction.');
 const kind=selected.targetType,context=identity(actor);
 type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome=await owner.transaction(async(database):Promise<Outcome>=>{
  const companyLocks=companyPurchaseReadLocks(database,actor.companyId);
  await lockPackageSettlementSources(database);
  await database.batch([...leases(database,actor.companyId,kind),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyLocks.slice(1));
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw attachmentReadChanged();
  const before=await image(database,actor.companyId,kind,source.targetId),current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
  const invalidAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor);
  const check=async(authorizeSource:boolean)=>{
   if(identity(actor)!==context)throw attachmentReadChanged();if(authorizeSource||initiallyCurrent)await requirePrivateAttachmentReaderCurrent(database,actor);else if(await privateFileDenialAuthorityWitness(database,actor)!==invalidAuthority)throw attachmentReadChanged();
   if(authorizeSource)await authorize(database,source);
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);
   if(after?.witness!==source.witness||await image(database,actor.companyId,kind,source.targetId)!==before)throw attachmentReadChanged();
  };
  const checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));let result:Result;
  try{result=await read(database);}catch(error){
   if(!checkpoint||!(error instanceof ApiError)||!([403,404].includes(error.status)||error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE'||error.status===503&&error.code==='FILE_UNAVAILABLE'))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false);
   const retained=(await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id);if(retained?.witness!==denied[0].witness)throw attachmentReadChanged();
   return {kind:'denied',error};
  }
  if(!before)throw attachmentReadChanged();
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);
  return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
