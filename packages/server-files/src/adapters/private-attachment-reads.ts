import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {iamMutationGuards} from '@erpx/server-identity_access/public/iam-store';
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {AttachmentComponentKind} from '@erpx/server-files/public/attachments/types';

/** A pool observation selects a fresh owned reader; it never authorizes bytes.
 * Draft sets and links are mutable, so every field must match after leases. */
export type AttachmentReadRoute={setId:string;targetType:string;targetId:string;purposeKey:string;componentKind:AttachmentComponentKind;componentRef:string|null};
export type AttachmentReadSource=AttachmentReadRoute&{witness:string};
const routeColumns='s.id "setId",s.target_type "targetType",s.target_id "targetId",s.purpose_key "purposeKey",s.component_kind "componentKind",s.component_ref "componentRef"';
export async function attachmentReadRoute(database:SqlDatabase,companyId:string,linkId:string):Promise<AttachmentReadRoute|null>{
 return database.prepare(`SELECT ${routeColumns} FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=?`).bind(linkId,companyId).first<AttachmentReadRoute>();
}
export async function attachmentReadSource(database:SqlDatabase,companyId:string,linkId:string):Promise<AttachmentReadSource|null>{
 return database.prepare(`SELECT ${routeColumns},jsonb_build_object('link',to_jsonb(l),'set',to_jsonb(s),'source',to_jsonb(a),'intent',to_jsonb(u),
 'scans',(SELECT jsonb_agg(to_jsonb(scan) ORDER BY scan.id) FROM attachment_scan_attempts scan WHERE scan.attachment_id=a.id))::text witness
 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
 LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
 LEFT JOIN upload_intents u ON u.id=coalesce(l.upload_intent_id,a.upload_id) AND u.company_id=s.company_id
 WHERE l.id=? AND s.company_id=?`).bind(linkId,companyId).first<AttachmentReadSource>();
}
export function attachmentPrivateReadSourceLocks(database:SqlDatabase){
 // An access audit's deferred effects can write these relations. SRE avoids
 // cross-Company SHARE-to-writer upgrade deadlocks, without a data snapshot.
 return database.prepare('LOCK TABLE attachment_links,attachment_sets,attachments,upload_intents,attachment_scan_attempts IN SHARE ROW EXCLUSIVE MODE');
}
export function attachmentReadChanged(){return new ApiError(409,'ATTACHMENT_FILE_READ_CHANGED','The private file routing, evidence or current authority changed while it was being read.');}
export function assertAttachmentReadRoute(selected:AttachmentReadRoute|null,actual:AttachmentReadRoute|null){
 if(!selected&&!actual)throw new ApiError(404,'ATTACHMENT_LINK_NOT_FOUND','File link not found.');
 if(!selected||!actual||(['setId','targetType','targetId','purposeKey','componentKind','componentRef'] as const).some(field=>selected[field]!==actual[field]))throw attachmentReadChanged();
}
export async function requirePrivateAttachmentReaderCurrent(database:SqlDatabase,actor:WorkContext){
 const key=`private-file-byte-read:${randomUUID()}`;
 try{await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);}
 catch(error){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==='object';depth++){const item=cause as {code?:string;cause?:unknown};if(item.code==='23514')throw attachmentReadChanged();cause=item.cause;}throw error;}
}
/** Called only after a lazy FileStore object was consumed inside the owned
 * transaction and all original source/policy/audit checks have succeeded. */
export function bufferedAttachmentRead<Result extends {object:StoredFile}>(result:Result,bytes:ArrayBuffer):Result{
 // Spreading a StoredFile evaluates its lazy body getter and can open an
 // unused original filesystem stream. Copy its declared metadata only.
 const {key,size,etag,httpEtag,uploaded,httpMetadata,customMetadata,range}=result.object;
 const object:StoredFile={key,size,etag,httpEtag,uploaded,httpMetadata,customMetadata,...(range?{range}:{}),body:new ReadableStream<Uint8Array>({start(controller){controller.enqueue(new Uint8Array(bytes));controller.close();}}),arrayBuffer:async()=>bytes.slice(0),text:async()=>new TextDecoder().decode(bytes)};
 return {...result,object};
}
/** Remaining owners retain their full original policy. This unit certifies
 * Files metadata/routing/IAM stability, not other owners' business sources. */
export async function withRemainingAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selectedRoute:AttachmentReadRoute|null,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'ATTACHMENT_FILE_READ_TRANSACTION_REQUIRED','Private bytes require their fresh owned read transaction.');
 return owner.transaction(async database=>{
  const locks=companyPurchaseReadLocks(database,actor.companyId);
  await database.batch([locks[0],attachmentPrivateReadSourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(locks.slice(1));
  const before=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selectedRoute,before);if(!before)throw attachmentReadChanged();
  const result=await read(database),bytes=await result.object.arrayBuffer();
  await flushFinanceDeferredEffects(database);await requirePrivateAttachmentReaderCurrent(database,actor);await authorize(database,before);
  const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(before,after);if(after?.witness!==before.witness)throw attachmentReadChanged();
  return bufferedAttachmentRead(result,bytes);
 });
}
