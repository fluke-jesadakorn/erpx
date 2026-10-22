import 'server-only';
import {isQualityAttachmentReadTarget,qualityAttachmentReadSourceLocks,qualityAttachmentReadSourceImage} from '@erpx/server-quality/adapters/attachment-read-sources';
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

const identity=(actor:WorkContext)=>JSON.stringify(actor);
function changed(){return new ApiError(409,'QUALITY_FILE_READ_CHANGED','The original Quality evidence, captured specification or source lineage or current account changed during its private read.');}
/** Native Quality holds/specifications retain their full original File
 * authority, included foundations and private document identity. */
export async function withQualityAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(!isQualityAttachmentReadTarget(selected.targetType))throw changed();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'QUALITY_FILE_READ_TRANSACTION_REQUIRED','Private Quality bytes require their complete fresh source-leased read transaction.');
 const context=identity(actor);type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome:Outcome=await owner.transaction(async database=>{
  const purchase=companyPurchaseReadLocks(database,actor.companyId);
  await lockPackageSettlementSources(database);
  await database.batch([...qualityAttachmentReadSourceLocks(database),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw changed();
  const before=await qualityAttachmentReadSourceImage(database,actor.companyId,source.targetType,source.targetId),currentScope=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${currentScope.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args).first();
  const deniedAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor),checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));
  const check=async(authorizeSource:boolean,denial=false)=>{
   if(identity(actor)!==context)throw changed();
   if(denial&&!initiallyCurrent){if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw changed();}
   else try{await requirePrivateAttachmentReaderCurrent(database,actor);}catch(error){if(error instanceof ApiError&&error.code==='ATTACHMENT_FILE_READ_CHANGED')throw changed();throw error;}
   if(authorizeSource)await authorize(database,source);
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);
   if(after?.witness!==source.witness||await qualityAttachmentReadSourceImage(database,actor.companyId,source.targetType,source.targetId)!==before)throw changed();
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
