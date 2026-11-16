import 'server-only';
import {randomUUID} from 'node:crypto';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {PortalActor} from '@erpx/server-legal/domain/portal';
import type {LegalKind} from '@erpx/server-legal/domain/model';
import {legalKindCapability} from '@erpx/server-legal/domain/purchases';
import {legalItemPurchasePredicate,inLegalOwnerUnit,deferLegalOwnerCheck,legalSourceWitness} from '@erpx/server-legal/adapters/purchases';
import {legalPackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {attachmentPrivateReadSourceLocks} from '@erpx/server-files/public/private-attachment-reads';
export async function legalPortalPurchasePredicate(alias='i',strictHistory=false){
 if(!strictHistory&&await legalPackageEnforcementMode()!=='enforced')return 'TRUE';
 const clock='legal_portal_purchase_clock.now_ms';return `EXISTS(WITH legal_portal_purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM legal_portal_purchase_clock WHERE ${legalItemPurchasePredicate(alias,'history',clock)} AND ${purchasedCapabilityPredicate(`${alias}.company_id`,'legal.documents','history',clock)} AND ${purchasedCapabilityPredicate(`${alias}.company_id`,'legal.access','history',clock)} AND ${scheduledFeaturePredicate(`${alias}.company_id`,'legal')})`;
}
export async function legalPortalSnapshotSourceImage(database:SqlDatabase,actor:PortalActor,id?:string){
 const row=await database.prepare(`SELECT COALESCE(jsonb_agg(jsonb_build_object(
 'share',to_jsonb(s),'item',to_jsonb(i),'document',to_jsonb(d),
 'settings',(SELECT to_jsonb(settings) FROM legal_settings settings WHERE settings.company_id=i.company_id),
 'destructions',(SELECT jsonb_agg(to_jsonb(destruction) ORDER BY destruction.id) FROM legal_destructions destruction WHERE destruction.item_id=i.id),
 'comments',(SELECT jsonb_agg(to_jsonb(comment) ORDER BY comment.id) FROM legal_comments comment WHERE comment.item_id=i.id AND comment.document_id=d.id AND comment.visibility='external'),
 'envelopes',(SELECT jsonb_agg(jsonb_build_object('envelope',to_jsonb(envelope),'signer',to_jsonb(signer)) ORDER BY envelope.id) FROM legal_envelopes envelope JOIN legal_signers signer ON signer.envelope_id=envelope.id WHERE envelope.item_id=i.id AND envelope.document_id=d.id AND signer.user_id=?),
 'billing',(SELECT jsonb_agg(to_jsonb(entry) ORDER BY entry.id) FROM legal_counsel_entries entry WHERE entry.item_id=i.id AND entry.created_by=?)
 ) ORDER BY s.id),'[]'::jsonb)::text image
 FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id
 WHERE lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND (?::text IS NULL OR s.id=?) AND s.revoked_at IS NULL
 AND s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND ${await legalPortalPurchasePredicate()}`)
 .bind(actor.userId,actor.userId,actor.email,actor.userId,id??null,id??null).first<{image:string}>();
 if(!row)throw new ApiError(503,'LEGAL_PORTAL_READ_UNAVAILABLE','The shared Legal collection could not be read.');
 return row.image;
}

async function share(database:SqlDatabase,actor:PortalActor,id:string){return database.prepare(`SELECT s.id,s.item_id,s.document_id,s.version,i.company_id,i.kind FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id WHERE s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>${packageClockSql}`).bind(id,actor.email,actor.userId).first<{id:string;item_id:string;document_id:string;version:number;company_id:string;kind:LegalKind}>();}
/** The external actor is authenticated by the normal verified-email Portal
 * context. Its actual share/token/source is revalidated inside the complete
 * existing owner unit; no Company assignment is fabricated for this user. */
export async function withLegalPortalPurchasedCommand<Result>(owner:SqlDatabase,actor:PortalActor,target:{shareId:string;key:string},run:(database:SqlDatabase)=>Promise<Result>,assertCurrent?:()=>Promise<void>){
 const mode=await legalPackageEnforcementMode();if(mode==='disabled'&&!assertCurrent)return run(owner);
 if(assertCurrent&&!isTransactionalSqlDatabase(owner))throw new ApiError(503,'LEGAL_PORTAL_TRANSACTION_REQUIRED','The current shared Legal command requires its original owned transaction.');
 const identified=await share(owner,actor,target.shareId);if(!identified)throw new ApiError(404,'LEGAL_SHARE_UNAVAILABLE','This share expired or is unavailable.');
 return inLegalOwnerUnit(owner,identified.company_id,async database=>{
  const before=await share(database,actor,target.shareId);if(!before||before.company_id!==identified.company_id||before.item_id!==identified.item_id||before.document_id!==identified.document_id)throw new ApiError(409,'LEGAL_SHARE_CHANGED','The actual shared Legal source changed.');
  const replay=!!await database.prepare('SELECT 1 FROM legal_portal_receipts WHERE id=? AND user_id=?').bind(target.key,actor.userId).first(),purpose=replay?'history' as const:'settle-existing' as const,scopes=[legalKindCapability(before.kind),'legal.documents','legal.access'].map(capability=>({companyId:before.company_id,capability,purpose}));
  const boundaries=scopes.flatMap(scope=>{const key=`legal-portal-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope);return boundary?[{key,boundary}]:[];});for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);await database.batch(boundaries.flatMap(item=>item.boundary.before));
  if(assertCurrent)await assertCurrent();
  const result=await run(database),expected=await legalSourceWitness(database,{companyId:before.company_id},before.item_id),expectedToken=await share(database,actor,target.shareId),expectedPortal=assertCurrent?await legalPortalSnapshotSourceImage(database,actor,target.shareId):null;if(!expectedToken)throw new ApiError(409,'LEGAL_SHARE_CHANGED','The current shared Legal source became unavailable.');
  deferLegalOwnerCheck(database,before.company_id,async()=>{const final=await share(database,actor,target.shareId);if(!final||final.company_id!==before.company_id||final.item_id!==before.item_id||final.document_id!==before.document_id||JSON.stringify(final)!==JSON.stringify(expectedToken)||expected!==await legalSourceWitness(database,{companyId:before.company_id},before.item_id))throw new ApiError(409,'LEGAL_SHARE_CHANGED','The shared Legal source or current token changed during its owner effects.');if(mode!=='disabled'&&!await database.prepare(`SELECT 1 FROM (SELECT ?::text company_id) legal_portal_release WHERE ${scheduledFeaturePredicate('legal_portal_release.company_id', 'legal')}`).bind(before.company_id).first())throw new ApiError(403,'LEGAL_SHARE_UNAVAILABLE','Legal is unavailable for this Company.');await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));if(assertCurrent){if(expectedPortal!==await legalPortalSnapshotSourceImage(database,actor,target.shareId))throw new ApiError(409,'LEGAL_SHARE_CHANGED','The completed shared Legal command source changed.');await assertCurrent();}});return result;
 },assertCurrent?async database=>{await database.batch([database.prepare('LOCK TABLE legal_settings,legal_destructions,legal_comments,legal_envelopes,legal_signers,legal_counsel_entries IN SHARE ROW EXCLUSIVE MODE'),attachmentPrivateReadSourceLocks(database)]);await lockScheduledFeatureSources(database);}:undefined);
}
export async function legalPortalPurchaseControls(database:SqlDatabase,actor:PortalActor,id:string){const mode=await legalPackageEnforcementMode();if(mode==='disabled')return;const source=await share(database,actor,id);if(!source)throw new ApiError(404,'LEGAL_SHARE_UNAVAILABLE','This share expired or is unavailable.');const {readPurchasedAccesses}=await import('@erpx/server-commercial/public/purchase-command-boundary'),codes=[legalKindCapability(source.kind),'legal.documents','legal.access'],access=await readPurchasedAccesses(database,mode,source.company_id,codes);return {settleExisting:codes.every(code=>access[code].settleExisting),history:codes.every(code=>access[code].history),revision:access[codes[0]].revision};}

export async function legalPortalReadWitness(database:SqlDatabase,actor:PortalActor,id:string){
 if(await legalPackageEnforcementMode()==='disabled')return null;
 const predicate=await legalPortalPurchasePredicate('i'),source=await share(database,actor,id);if(!source)throw new ApiError(404,'LEGAL_SHARE_UNAVAILABLE','This share expired or is unavailable.');
 const witness=await database.prepare(`SELECT jsonb_build_object('share',s.id,'shareVersion',s.version,'item',i.id,'company',i.company_id,'kind',i.kind,'document',d.id,'documentVersion',d.version,'checksum',d.checksum,'bytes',d.bytes,'link',d.attachment_link_id,'source',d.attachment_source_id,'object',d.object_key)::text image FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id WHERE s.id=? AND ${predicate}`).bind(id).first<string>('image');if(!witness)throw new ApiError(404,'LEGAL_SHARE_UNAVAILABLE','This shared Legal source is unavailable.');return witness;
}
