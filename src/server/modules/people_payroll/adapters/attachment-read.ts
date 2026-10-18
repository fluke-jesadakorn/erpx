import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '../../identity_access/public/private-file-denial-authority';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {companyPurchaseReadLocks} from '../../commercial/public/purchase-command-boundary';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,attachmentReadChanged,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '../../files/public/private-attachment-reads';
import {attachmentDeniedReadAudits} from '../../files/public/attachment-denied-read-audits';
import {isPeopleAttachmentReadTarget,peopleAttachmentReadSourceLocks,peopleAttachmentReadSourceImage} from './attachment-read-sources';
const identity=(actor:WorkContext)=>JSON.stringify(actor);
/** Exact native People/Payroll owners keep their original complete File policy and
 * actual included-foundation purchase decisions. This constructor adds no People
 * role or purchased capability and cannot inherit an arbitrary active parent. */
export async function withPeopleAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(!isPeopleAttachmentReadTarget(selected.targetType))throw attachmentReadChanged();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'PEOPLE_FILE_READ_TRANSACTION_REQUIRED','Private People evidence requires its original complete fresh read transaction.');
 const target=selected.targetType,context=identity(actor);
 type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome=await owner.transaction(async(database):Promise<Outcome>=>{
  // Strong package/source fence excludes normal enabled writers before their
  // Company advisory lock. No SELECT or owner snapshot precedes these leases.
  await lockPackageSettlementSources(database);
  await database.batch([...peopleAttachmentReadSourceLocks(database),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw attachmentReadChanged();
  const before=await peopleAttachmentReadSourceImage(database,actor.companyId,target,source.targetId),current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
  const invalidAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor);
  const check=async(authorizeSource:boolean)=>{
   if(identity(actor)!==context)throw attachmentReadChanged();if(authorizeSource||initiallyCurrent)await requirePrivateAttachmentReaderCurrent(database,actor);else if(await privateFileDenialAuthorityWitness(database,actor)!==invalidAuthority)throw attachmentReadChanged();if(authorizeSource)await authorize(database,source);
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);
   if(after?.witness!==source.witness||await peopleAttachmentReadSourceImage(database,actor.companyId,target,source.targetId)!==before)throw attachmentReadChanged();
  };
  const checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));let result:Result;
  try{result=await read(database);}catch(error){
   if(!checkpoint||!(error instanceof ApiError)||!([403,404].includes(error.status)||error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE'||error.status===503&&error.code==='FILE_UNAVAILABLE'))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false);
   const retained=(await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id);if(retained?.witness!==denied[0].witness)throw attachmentReadChanged();
   return {kind:'denied',error};
  }
  if(!before)throw attachmentReadChanged();const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);
  return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
