import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {attachmentReadSource,assertAttachmentReadRoute,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '@erpx/server-files/public/private-attachment-reads';
import {attachmentDeniedReadAudits} from '@erpx/server-files/public/attachment-denied-read-audits';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {privateFileDenialAuthorityWitness} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {messagingSourceReadLocks,requireMessagingPurchase} from '@erpx/server-collaboration/adapters/messaging-purchases';
async function image(database:SqlDatabase,actor:WorkContext,id:string,linkId:string){return database.prepare(`SELECT jsonb_build_object('room',to_jsonb(r),'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_room_members m WHERE m.room_id=r.id),'team',(SELECT to_jsonb(t) FROM messaging_teams t WHERE t.id=r.team_id AND t.company_id=r.company_id),'teamMembers',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members m WHERE m.team_id=r.team_id),'file',(SELECT jsonb_agg(jsonb_build_object('file',to_jsonb(f),'message',(SELECT to_jsonb(msg) FROM messaging_messages msg WHERE msg.id=f.message_id AND msg.room_id=r.id)) ORDER BY f.id) FROM messaging_files f WHERE f.room_id=r.id AND f.attachment_link_id=?))::text image FROM messaging_rooms r WHERE r.id=? AND r.company_id=?`).bind(linkId,id,actor.companyId).first<string>('image');}
async function leases(database:SqlDatabase,companyId:string){await messagingSourceReadLocks(database,companyId,true);}
export async function withMessagingAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(selected.targetType!=='messaging_room')throw new ApiError(409,'MESSAGING_FILE_READ_CHANGED','The original private conversation file route changed.');
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'MESSAGING_FILE_READ_TRANSACTION_REQUIRED','Private conversation bytes require their complete source-leased read transaction.');
 const context=JSON.stringify(actor);
 type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const execute=async(database:SqlDatabase):Promise<Outcome>=>{
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw new ApiError(404,'ATTACHMENT_LINK_NOT_FOUND','File link not found.');
  await requireMessagingPurchase(database,actor,'history');const before=await image(database,actor,source.targetId,linkId);if(!before)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original private conversation is unavailable.');
  const check=async(authorizeSource:boolean,denial=false)=>{if(JSON.stringify(actor)!==context)throw new ApiError(409,'MESSAGING_FILE_READ_CHANGED','The captured private conversation account changed.');if(denial&&!initiallyCurrent){if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw new ApiError(409,'MESSAGING_FILE_READ_CHANGED','The rejected original account changed during its access audit.');}else{await requirePrivateAttachmentReaderCurrent(database,actor);await requireMessagingPurchase(database,actor,'history');}if(authorizeSource)await authorize(database,source);const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);if(after?.witness!==source.witness||await image(database,actor,source.targetId,linkId)!==before)throw new ApiError(409,'MESSAGING_FILE_READ_CHANGED','The private conversation source or evidence changed during its access audit.');};
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
  const deniedAuthority=!initiallyCurrent?await privateFileDenialAuthorityWitness(database,actor):null;
  const checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));let result:Result;
  try{result=await read(database);}catch(error){
   // The original reader records a policy denial before throwing. Commit only
   // that actual event, with no bytes, after all late source/IAM checks. A late
   // failure following a successful read still rolls the allowed audit back.
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));
   if(!(error instanceof ApiError)||![403,404,423,503].includes(error.status)||denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false,true);
   const retained=(await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id);if(retained?.witness!==denied[0].witness)throw new ApiError(409,'MESSAGING_FILE_READ_CHANGED','The original private-file denial audit changed.');
   return {kind:'denied',error};
  }
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 };
 const outcome=await owner.transaction(async database=>{await leases(database,actor.companyId);return execute(database);});if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
