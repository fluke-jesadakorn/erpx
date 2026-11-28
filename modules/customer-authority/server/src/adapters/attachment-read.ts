import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '@erpx/identity-access-server/public/private-file-denial-authority';
import {currentAssignmentReadScope} from '@erpx/identity-access-server/public/record-source-authority';
import {attachmentDeniedReadAudits} from '@erpx/files-server/public/attachment-denied-read-audits';
import {lockQueuedCommandAuthority} from '@erpx/identity-access-server/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/pms-server/public/scheduled-feature-source-locks';
import {companyPurchaseReadLocks} from '@erpx/commercial-server/public/purchase-command-boundary';
import {commerceCompleteOwnerSourceReadLocks} from '@erpx/commerce-server/public/owner-source-locks';
import {flushFinanceDeferredEffects} from '@erpx/finance-server/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,attachmentReadChanged,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '@erpx/files-server/public/private-attachment-reads';
import {customerPortalSourceReadLocks} from '@erpx/customer-authority-server/adapters/portal-source-locks';
import {bindingIdentityCurrentSql} from '@erpx/customer-authority-server/adapters/attachment-withdraw-proof';
import {existenceProbe} from './customer-authority-read-tail-r30-sources';
import {companyScopeSelect} from './customer-authority-read-tail-r33-sources';
import {portalPurchaseBindingSource} from './portal-purchase-binding-source';
import {jsonbAggregateHead} from './customer-authority-read-tail-r45-sources';

export async function portalAttachmentSourceWitness(database:SqlDatabase,actor:WorkContext,id:string){
 return database.prepare(`${portalPurchaseBindingSource(`jsonb_build_object('binding',to_jsonb(b),'order',to_jsonb(o),
 'identities',${jsonbAggregateHead('i) ORDER BY i.user_id) FROM customer_access_identities i WHERE i.binding_id=b.id)')})::text image`)}
 WHERE b.id=? AND b.company_id=?`).bind(id,actor.companyId).first<string>('image');
}
/** Verified external customers keep binding/identity/expiry policy; scoped
 * staff keep retained history policy. Neither receives fabricated authority.
 * Original pre-1053 Portal sources are sufficient for private evidence reads. */
export async function withPortalAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selectedRoute:AttachmentReadRoute,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>,assertRequestAuthority?:()=>Promise<void>):Promise<Result>{
 if(selectedRoute.targetType!=='customer_access_binding')throw attachmentReadChanged();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'PORTAL_FILE_READ_TRANSACTION_REQUIRED','Private Portal bytes require their complete fresh owned read transaction.');
 const context=JSON.stringify(actor);
 type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome=await owner.transaction(async(database):Promise<Outcome>=>{
  const locks=companyPurchaseReadLocks(database,actor.companyId);
  // Match the original public Portal's package fence before shared sources.
  await database.batch([database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),...commerceCompleteOwnerSourceReadLocks(database),...customerPortalSourceReadLocks(database),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(locks.slice(1));
  const before=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selectedRoute,before);if(!before)throw attachmentReadChanged();
  const image=await portalAttachmentSourceWitness(database,actor,before.targetId);if(!image)throw new ApiError(404,'PORTAL_ATTACHMENT_TARGET_NOT_FOUND','The original Portal evidence owner is unavailable.');
  const external=actor.role==='external_customer'&&actor.assignmentId===`customer-portal:${before.targetId}`,current=currentAssignmentReadScope(actor,'workforce_actor_scope');
  const initiallyCurrent=external?!!await database.prepare(bindingIdentityCurrentSql).bind(before.targetId,actor.companyId,actor.userId,actor.email.toLowerCase(),actor.email.toLowerCase()).first():!!await database.prepare(existenceProbe(`FROM(${companyScopeSelect(',?::text branch_id,?::text department_id')})workforce_actor_scope`,current.where)).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
  const invalidAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor);
  const check=async(authorizeSource:boolean)=>{
   await assertRequestAuthority?.();
   if(JSON.stringify(actor)!==context)throw attachmentReadChanged();
   if(!external&&(authorizeSource||initiallyCurrent))await requirePrivateAttachmentReaderCurrent(database,actor);else if(!authorizeSource&&!initiallyCurrent&&await privateFileDenialAuthorityWitness(database,actor)!==invalidAuthority)throw attachmentReadChanged();
   if(authorizeSource)await authorize(database,before);
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(before,after);
   if(after?.witness!==before.witness||await portalAttachmentSourceWitness(database,actor,before.targetId)!==image)throw attachmentReadChanged();
  };
  const checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));let result:Result;
  try{result=await read(database);}catch(error){
   if(!checkpoint||!(error instanceof ApiError)||!([403,404].includes(error.status)||error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE'||error.status===503&&error.code==='FILE_UNAVAILABLE'))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false);
   const retained=(await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id);if(retained?.witness!==denied[0].witness)throw attachmentReadChanged();
   return {kind:'denied',error};
  }
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);
  return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
