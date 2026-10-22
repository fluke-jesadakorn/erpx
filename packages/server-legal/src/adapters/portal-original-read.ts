import 'server-only';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {holdingPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {attachmentPrivateReadSourceLocks,attachmentReadSource} from '@erpx/server-files/public/private-attachment-reads';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import type {PortalActor} from '@erpx/server-legal/domain/portal';
import {legalPortalPurchasePredicate} from '@erpx/server-legal/adapters/portal-purchases';

async function originalImage(database:SqlDatabase,actor:PortalActor,id:string){
 const row=await database.prepare(`SELECT i.company_id,d.attachment_link_id,
 jsonb_build_object('share',to_jsonb(s),'item',to_jsonb(i),'document',to_jsonb(d),
 'settings',(SELECT to_jsonb(settings) FROM legal_settings settings WHERE settings.company_id=i.company_id),
 'destructions',(SELECT jsonb_agg(to_jsonb(destruction) ORDER BY destruction.id) FROM legal_destructions destruction WHERE destruction.item_id=i.id),
 'comments',(SELECT jsonb_agg(to_jsonb(comment) ORDER BY comment.id) FROM legal_comments comment WHERE comment.item_id=i.id AND comment.document_id=d.id AND comment.visibility='external'),
 'envelopes',(SELECT jsonb_agg(jsonb_build_object('envelope',to_jsonb(envelope),'signer',to_jsonb(signer)) ORDER BY envelope.id) FROM legal_envelopes envelope JOIN legal_signers signer ON signer.envelope_id=envelope.id WHERE envelope.item_id=i.id AND envelope.document_id=d.id AND signer.user_id=?),
 'billing',(SELECT jsonb_agg(to_jsonb(entry) ORDER BY entry.id) FROM legal_counsel_entries entry WHERE entry.item_id=i.id AND entry.created_by=?))::text image
 FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id
 WHERE s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL
 AND s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND ${await legalPortalPurchasePredicate()}`)
 .bind(actor.userId,actor.userId,id,actor.email,actor.userId).first<{company_id:string;attachment_link_id:string|null;image:string}>();
 if(!row)throw new ApiError(404,'LEGAL_SHARE_UNAVAILABLE','This share expired or is unavailable.');
 const file=row.attachment_link_id?await attachmentReadSource(database,row.company_id,row.attachment_link_id):null;
 return {image:JSON.stringify([row,file]),linkId:row.attachment_link_id};
}
/** External shares retain their own verified actor; no Company assignment is
 * fabricated. Buffering and both original access audits belong to this fresh
 * Holding read. Control-session composition is a separate request concern. */
export async function withLegalPortalOriginalRead<Result>(owner:SqlDatabase,actor:PortalActor,id:string,read:(database:SqlDatabase)=>Promise<Result>,assertCurrent?:()=>Promise<void>){
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'LEGAL_PORTAL_READ_TRANSACTION_REQUIRED','Shared Legal originals require their fresh owned read.');
 const context=JSON.stringify(actor);
 type Outcome={kind:"read";result:Result}|{kind:"denied";error:ApiError};
 const outcome=await owner.transaction(async(database):Promise<Outcome>=>{
  const purchaseLocks=holdingPurchaseReadLocks(database);
  await database.batch([purchaseLocks[0],database.prepare('LOCK TABLE legal_items,legal_members,legal_documents,legal_shares,legal_settings,legal_destructions,legal_comments,legal_envelopes,legal_signers,legal_counsel_entries IN SHARE ROW EXCLUSIVE MODE'),attachmentPrivateReadSourceLocks(database)]);
  await lockScheduledFeatureSources(database);await database.batch(purchaseLocks.slice(1));
  const before=await originalImage(database,actor,id);
  const audits=async()=> (await database.prepare('SELECT id,outcome,reason,to_jsonb(event)::text image FROM attachment_access_events event WHERE actor_id=? AND link_id=? ORDER BY id').bind(actor.userId,before.linkId).all<{id:string;outcome:string;reason:string|null;image:string}>()).results;
  const checkpoint=new Set((await audits()).map(event=>event.id));
  const check=async()=>{
   await flushFinanceDeferredEffects(database);
   if(JSON.stringify(actor)!==context||(await originalImage(database,actor,id)).image!==before.image)throw new ApiError(409,'LEGAL_SHARE_CHANGED','The current shared Legal original changed.');
   if(assertCurrent)await assertCurrent();
  };
  let result:Result;
  try{result=await read(database);}catch(error){
   // Retain only the one original, currently authorized File denial. Never
   // commit an allowed audit after a failed or expired share continuation.
   if(!(error instanceof ApiError)||!(error.status===503&&error.code==='FILE_UNAVAILABLE'||error.status===409&&error.code==='LEGAL_INTEGRITY'))throw error;
   const captured=await audits(),added=captured.filter(event=>!checkpoint.has(event.id));
   if(added.length!==1||added[0].outcome!=='denied'||added[0].reason!==error.code)throw error;
   await check();
   if(JSON.stringify(await audits())!==JSON.stringify(captured))throw new ApiError(409,'LEGAL_SHARE_CHANGED','The current shared Legal file audit changed.');
   return {kind:'denied',error};
  }
  await check();return {kind:'read',result};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
