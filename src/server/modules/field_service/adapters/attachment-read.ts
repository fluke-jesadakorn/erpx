import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '../../identity_access/public/private-file-denial-authority';
import {companyPurchaseReadLocks} from '../../commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {fieldServiceTicketSourceReadLocks} from '../../support/public/field-service-sources';
import {fieldServiceAssetAttachmentSourceLocks} from '../../assets/public/field-service-references';
import {fieldServiceCustomerAttachmentSourceLocks} from '../../finance/public/field-service-references';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {attachmentDeniedReadAudits} from '../../files/public/attachment-denied-read-audits';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '../../files/public/private-attachment-reads';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {serviceOwnerSourceImage} from './purchases';

function changed(){return new ApiError(409,'SERVICE_FILE_READ_CHANGED','The original Service visit, evidence or selected authority changed during its private read.');}
/** The actual registered Visit is the sole entry. Full original Files policy
 * still checks module, record, purpose, purchases and the linked target. No
 * caller transaction may inherit a private source snapshot. */
export async function withServiceAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(selected.targetType!=='field_service_visit')throw changed();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'SERVICE_FILE_READ_TRANSACTION_REQUIRED','Private Service bytes require their complete fresh owned read transaction.');
 const context=JSON.stringify(actor);type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome:Outcome=await owner.transaction(async database=>{
  // No advisory/data SELECT may precede these native/Files/IAM leases. The
  // Commercial writer fence excludes commands holding a Company advisory.
  await lockPackageSettlementSources(database);
  await database.batch([...fieldServiceTicketSourceReadLocks(database),fieldServiceAssetAttachmentSourceLocks(database),fieldServiceCustomerAttachmentSourceLocks(database),database.prepare('LOCK TABLE field_service_agreements,field_service_agreement_events,field_service_orders,field_service_order_events,field_service_part_reservations,field_service_billing_releases IN SHARE ROW EXCLUSIVE MODE'),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw changed();
  const snapshot=()=>serviceOwnerSourceImage(database,actor,{kind:'order',id:source.targetId}),before=(await snapshot())?.image??null;
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first(),deniedAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor),checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));
  const check=async(authorizeSource:boolean,denial=false)=>{
   if(JSON.stringify(actor)!==context)throw changed();
   if(denial&&!initiallyCurrent){if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw changed();}
   else await requirePrivateAttachmentReaderCurrent(database,actor);
   if(authorizeSource){if(!before)throw changed();await authorize(database,source);}
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);if(after?.witness!==source.witness||((await snapshot())?.image??null)!==before)throw changed();
  };
  let result:Result;
  try{result=await read(database);}catch(error){
   if(!(error instanceof ApiError)||![403,404,423,503].includes(error.status))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false,true);
   if((await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id)?.witness!==denied[0].witness)throw changed();return {kind:'denied',error};
  }
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
