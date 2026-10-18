import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '../../identity_access/public/private-file-denial-authority';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {isPmsAttachmentReadTarget,pmsAttachmentReadSourceLocks,pmsAttachmentReadSourceImage} from '../../pms/public/attachment-read-sources';
import {isPosAttachmentReadTarget,posAttachmentReadSourceLocks,posAttachmentReadSourceImage} from '../../pos/public/attachment-read-sources';
import {companyPurchaseReadLocks} from '../../commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {attachmentDeniedReadAudits} from './attachment-denied-read-audits';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from './private-attachment-reads';
export function isHospitalityAttachmentReadTarget(target:string){return isPmsAttachmentReadTarget(target)||isPosAttachmentReadTarget(target);}
function changed(){return new ApiError(409,'HOSPITALITY_FILE_READ_CHANGED','The original hotel or POS evidence source changed during its private read.');}
const identity=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.userId,actor.email,actor.assignmentId,actor.role,actor.branchId,actor.departmentId,actor.assignmentScope,actor.scope,actor.modules,actor.authority??null,actor.iam??null]);
function image(database:SqlDatabase,actor:WorkContext,source:AttachmentReadRoute){
 if(isPmsAttachmentReadTarget(source.targetType))return pmsAttachmentReadSourceImage(database,actor.companyId,source.targetType,source.targetId);
 if(isPosAttachmentReadTarget(source.targetType))return posAttachmentReadSourceImage(database,actor.companyId,source.targetType,source.targetId);
 throw changed();
}
/** A logical existing link may legitimately reuse a File whose immutable
 * source has another owner. The selected link/set, complete File witness and
 * original linked-target policy determine this read; no source is relabelled. */
export async function withHospitalityAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(!isHospitalityAttachmentReadTarget(selected.targetType))throw changed();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'HOSPITALITY_FILE_READ_TRANSACTION_REQUIRED','Private hotel and POS bytes require their complete fresh source-leased read transaction.');
 const context=identity(actor);type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome:Outcome=await owner.transaction(async database=>{
  const purchase=companyPurchaseReadLocks(database,actor.companyId),sourceLock=isPmsAttachmentReadTarget(selected.targetType)?pmsAttachmentReadSourceLocks(database):posAttachmentReadSourceLocks(database);
  // Enabled original commands take their Company advisory before owner writes.
  // Exclude those writers before our source leases so neither can retain the
  // other's advisory/source lock while waiting for its first snapshot.
  await lockPackageSettlementSources(database);
  await database.batch([sourceLock,attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw changed();
  const before=await image(database,actor,source);if(!before)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original hotel or POS owner is unavailable.');
  const currentScope=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${currentScope.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args).first();
  const deniedAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor),checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));
  const check=async(authorizeSource:boolean,denial=false)=>{
   if(identity(actor)!==context)throw changed();
   if(denial&&!initiallyCurrent){if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw changed();}
   else try{await requirePrivateAttachmentReaderCurrent(database,actor);}catch(error){if(error instanceof ApiError&&error.code==='ATTACHMENT_FILE_READ_CHANGED')throw changed();throw error;}
   if(authorizeSource)await authorize(database,source);
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);
   if(after?.witness!==source.witness||await image(database,actor,source)!==before)throw changed();
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
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
