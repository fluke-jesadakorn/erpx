import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {supportOrderAttachmentSourceLocks,supportOrderAttachmentSourceImage} from '@erpx/server-commerce/public/support-attachment-sources';
import {fieldServiceAssetAttachmentSourceLocks,fieldServiceAssetAttachmentSourceImage} from '@erpx/server-assets/public/field-service-references';
import {fieldServiceCustomerAttachmentSourceLocks,fieldServiceCustomerAttachmentSourceImage} from '@erpx/server-finance/public/field-service-references';
import {posAttachmentReadSourceLocks,posAttachmentReadSourceImage} from '@erpx/server-pos/public/attachment-read-sources';
import {socialPmsAttachmentSourceReadLocks,socialPmsSourceFacts} from '@erpx/server-pms/public/social-source-facts';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {attachmentDeniedReadAudits} from '@erpx/server-files/public/attachment-denied-read-audits';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '@erpx/server-files/public/private-attachment-reads';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';

async function image(database:SqlDatabase,companyId:string,id:string){
 const ticket=await database.prepare('SELECT to_jsonb(t)::text image,t.source_record_json,t.order_id,t.customer_id FROM support_tickets t WHERE t.id=? AND t.company_id=?').bind(id,companyId).first<{image:string;source_record_json:string|null;order_id:string|null;customer_id:string|null}>();if(!ticket)return null;
 let origin:string|null=null;
 if(ticket.source_record_json){const source:unknown=JSON.parse(ticket.source_record_json);if(source&&typeof source==='object'&&'id' in source&&typeof source.id==='string'&&'kind' in source){
  if(source.kind==='pos_sale')origin=await posAttachmentReadSourceImage(database,companyId,'pos_sale',source.id);
  else if(source.kind==='asset')origin=await fieldServiceAssetAttachmentSourceImage(database,companyId,source.id);
  else if(source.kind==='pms_record')origin=await database.prepare(`SELECT jsonb_agg(to_jsonb(source) ORDER BY source.kind)::text image FROM (${socialPmsSourceFacts}) source WHERE source.id=? AND source.company_id=?`).bind(source.id,companyId).first<string>('image');
 }}
 return JSON.stringify([ticket.image,origin,ticket.order_id?await supportOrderAttachmentSourceImage(database,companyId,ticket.order_id):null,ticket.customer_id?await fieldServiceCustomerAttachmentSourceImage(database,companyId,ticket.customer_id):null]);
}
function changed(){return new ApiError(409,'SUPPORT_FILE_READ_CHANGED','The original Helpdesk case, linked source, evidence or selected authority changed during its private read.');}
/** Only support_ticket is registered for Files. Incidents/SLA/sessions keep
 * their existing ticket relationships; this invents no extra File target. */
export async function withSupportAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(selected.targetType!=='support_ticket')throw changed();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'SUPPORT_FILE_READ_TRANSACTION_REQUIRED','Private Helpdesk bytes require their complete fresh owned read transaction.');
 const context=JSON.stringify(actor);type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome:Outcome=await owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  await database.batch([database.prepare('LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE'),supportOrderAttachmentSourceLocks(database),fieldServiceAssetAttachmentSourceLocks(database),fieldServiceCustomerAttachmentSourceLocks(database),posAttachmentReadSourceLocks(database),socialPmsAttachmentSourceReadLocks(database),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw changed();const before=await image(database,actor.companyId,source.targetId);
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first(),deniedAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor),checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));
  const check=async(authorizeSource:boolean,denial=false)=>{
   if(JSON.stringify(actor)!==context)throw changed();
   if(denial&&!initiallyCurrent){if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw changed();}else await requirePrivateAttachmentReaderCurrent(database,actor);
   if(authorizeSource){if(!before)throw changed();await authorize(database,source);}
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);if(after?.witness!==source.witness||await image(database,actor.companyId,source.targetId)!==before)throw changed();
  };
  let result:Result;try{result=await read(database);}catch(error){
   if(!(error instanceof ApiError)||![403,404,423,503].includes(error.status))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false,true);if((await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id)?.witness!==denied[0].witness)throw changed();return {kind:'denied',error};
  }
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
