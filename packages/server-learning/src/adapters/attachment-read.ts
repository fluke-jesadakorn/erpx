import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {attachmentDeniedReadAudits} from '@erpx/server-files/public/attachment-denied-read-audits';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '@erpx/server-files/public/private-attachment-reads';

export function isLearningAttachmentReadTarget(target:string){return target==='learning_course'||target==='learning_assignment';}
const identity=(actor:WorkContext)=>JSON.stringify(actor);
function changed(){return new ApiError(409,'LEARNING_FILE_READ_CHANGED','The original course, learner evidence or current account changed during its private read.');}
function image(database:SqlDatabase,companyId:string,type:string,id:string){
 return database.prepare(`SELECT jsonb_build_object('course',to_jsonb(course),'assignments',
 (SELECT jsonb_agg(jsonb_build_object('assignment',to_jsonb(assignment),
 'handoff',(SELECT to_jsonb(handoff) FROM learning_eligibility_handoffs handoff WHERE handoff.assignment_id=assignment.id AND handoff.company_id=assignment.company_id),
 'certificates',(SELECT jsonb_agg(jsonb_build_object('certificate',to_jsonb(certificate),'revocation',to_jsonb(revocation)) ORDER BY certificate.id)
 FROM learning_certificates certificate LEFT JOIN learning_certificate_revocations revocation ON revocation.certificate_id=certificate.id
 WHERE certificate.assignment_id=assignment.id AND certificate.company_id=assignment.company_id)) ORDER BY assignment.id)
 FROM learning_assignments assignment WHERE assignment.course_id=course.id AND assignment.company_id=course.company_id AND (?::text='learning_course' OR assignment.id=?)))::text image
 FROM learning_courses course WHERE course.company_id=? AND (?::text='learning_course' AND course.id=? OR ?::text='learning_assignment' AND EXISTS(SELECT 1 FROM learning_assignments selected WHERE selected.id=? AND selected.course_id=course.id AND selected.company_id=course.company_id))`).bind(type,id,companyId,type,id,type,id).first<string>('image');
}
/** Learners keep their original self/material access and certificate identity.
 * No full People package, HR module or extra document-manager role is added. */
export async function withLearningAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(!isLearningAttachmentReadTarget(selected.targetType))throw changed();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'LEARNING_FILE_READ_TRANSACTION_REQUIRED','Private learning bytes require their complete fresh source-leased read transaction.');
 const context=identity(actor);type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome:Outcome=await owner.transaction(async database=>{
  const purchase=companyPurchaseReadLocks(database,actor.companyId);
  await lockPackageSettlementSources(database);
  await database.batch([database.prepare('LOCK TABLE learning_courses,learning_assignments,learning_eligibility_handoffs,learning_certificates,learning_certificate_revocations IN SHARE ROW EXCLUSIVE MODE'),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw changed();
  const before=await image(database,actor.companyId,source.targetType,source.targetId),currentScope=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${currentScope.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args).first();
  const deniedAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor),checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));
  const check=async(authorizeSource:boolean,denial=false)=>{
   if(identity(actor)!==context)throw changed();
   if(denial&&!initiallyCurrent){if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw changed();}
   else try{await requirePrivateAttachmentReaderCurrent(database,actor);}catch(error){if(error instanceof ApiError&&error.code==='ATTACHMENT_FILE_READ_CHANGED')throw changed();throw error;}
   if(authorizeSource)await authorize(database,source);
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);
   if(after?.witness!==source.witness||await image(database,actor.companyId,source.targetType,source.targetId)!==before)throw changed();
  };
  let result:Result;
  try{result=await read(database);}catch(error){
   if(!(error instanceof ApiError)||!([403,404].includes(error.status)||(error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE')||(error.status===503&&error.code==='FILE_UNAVAILABLE')))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));
   if(denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false,true);
   if((await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id)?.witness!==denied[0].witness)throw changed();
   return {kind:'denied',error};
  }
  if(!before)throw changed();
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
