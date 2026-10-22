import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {attachmentDeniedReadAudits} from '@erpx/server-files/public/attachment-denied-read-audits';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {commerceCompleteOwnerSourceReadLocks} from '@erpx/server-commerce/public/owner-source-locks';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,attachmentReadChanged,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '@erpx/server-files/public/private-attachment-reads';
import {ecommerceCommerceSourceReadLocks} from '@erpx/server-ecommerce/adapters/commerce-source-locks';

async function ecommerceImage(database:SqlDatabase,actor:WorkContext,id:string){
 return database.prepare(`SELECT jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image
 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN commerce_products p ON p.id=l.product_id AND p.company_id=c.company_id WHERE l.id=? AND c.company_id=?`).bind(id,actor.companyId).first<string>('image');
}
/** Actual listing/channel/product sources remain leased through lazy bytes and
 * original current read policy. Listing media never grants a Sales/Stock role. */
export async function withEcommerceAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selectedRoute:AttachmentReadRoute,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(selectedRoute.targetType!=='ecommerce_listing')throw attachmentReadChanged();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'ECOMMERCE_FILE_READ_TRANSACTION_REQUIRED','Private Ecommerce bytes require their complete fresh owned read transaction.');
 const context=JSON.stringify(actor);
 type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome=await owner.transaction(async(database):Promise<Outcome>=>{
  const locks=companyPurchaseReadLocks(database,actor.companyId);
  await database.batch([locks[0],...commerceCompleteOwnerSourceReadLocks(database),...ecommerceCommerceSourceReadLocks(database),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(locks.slice(1));
  const before=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selectedRoute,before);if(!before)throw attachmentReadChanged();
  const image=await ecommerceImage(database,actor,before.targetId);if(!image)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original Ecommerce listing evidence owner is unavailable.');
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
  const invalidAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor);
  const check=async(authorizeSource:boolean)=>{
   if(JSON.stringify(actor)!==context)throw attachmentReadChanged();
   if(authorizeSource||initiallyCurrent)await requirePrivateAttachmentReaderCurrent(database,actor);else if(await privateFileDenialAuthorityWitness(database,actor)!==invalidAuthority)throw attachmentReadChanged();if(authorizeSource)await authorize(database,before);
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(before,after);
   if(after?.witness!==before.witness||await ecommerceImage(database,actor,before.targetId)!==image)throw attachmentReadChanged();
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
