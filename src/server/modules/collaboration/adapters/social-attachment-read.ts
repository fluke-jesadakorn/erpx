import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {privateFileDenialAuthoritySourceLocks,privateFileDenialAuthorityWitness} from '../../identity_access/public/private-file-denial-authority';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {projectSocialAttachmentSourceReadLocks} from '../../projects/public/social-purchase-sources';
import {socialPmsAttachmentSourceReadLocks} from '../../pms/public/social-source-facts';
import {lockWorkOperationsSettlementSources} from '../../operations/public/work-package-obligation-sources';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {companyPurchaseReadLocks} from '../../commercial/public/purchase-command-boundary';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '../../files/public/private-attachment-reads';
import {attachmentDeniedReadAudits} from '../../files/public/attachment-denied-read-audits';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {workSocialAttachmentSourceImage} from './work-social-purchases';
const identity=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.email,actor.assignmentId,actor.role,actor.scope,actor.assignmentScope,actor.modules,actor.authority??null,actor.iam??null]);
function changed(){return new ApiError(409,'SOCIAL_FILE_READ_CHANGED','The private conversation file, its native source or current authority changed during its read.');}
export function isSocialAttachmentReadTarget(type:string):type is 'social_draft'|'social_post'{return type==='social_draft'||type==='social_post';}
async function leases(database:SqlDatabase,companyId:string){
 // Existing enabled command owners take a Company advisory before native
 // writes. The global settlement fence comes FIRST, excluding that lock cycle.
 await lockPackageSettlementSources(database);await lockWorkOperationsSettlementSources(database);
 await database.batch([database.prepare('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins,social_post_drafts,social_post_participants,social_handover_acknowledgements IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE messaging_teams,messaging_team_members IN SHARE ROW EXCLUSIVE MODE'),projectSocialAttachmentSourceReadLocks(database),socialPmsAttachmentSourceReadLocks(database),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,companyId).slice(1));
}
/** Fresh exact-owner read only. Original Files policy runs before any FileStore
 * access, and again after lazy bytes plus access-audit deferred effects. */
export async function withSocialAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(!isSocialAttachmentReadTarget(selected.targetType))throw changed();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'SOCIAL_FILE_READ_TRANSACTION_REQUIRED','Private conversation bytes require their fresh complete source-leased owner read.');
 const targetType=selected.targetType,context=identity(actor);type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome=await owner.transaction<Outcome>(async database=>{
  await leases(database,actor.companyId);const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw changed();
  const before=await workSocialAttachmentSourceImage(database,actor.companyId,targetType,source.targetId);
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
  const deniedAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor),checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));
  const check=async(success:boolean)=>{
   if(identity(actor)!==context)throw changed();
   if(success||initiallyCurrent)await requirePrivateAttachmentReaderCurrent(database,actor);else if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw changed();
   if(success)await authorize(database,source);
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);
   if(after?.witness!==source.witness||before!==await workSocialAttachmentSourceImage(database,actor.companyId,targetType,source.targetId))throw changed();
  };
  let result:Result;
  try{result=await read(database);}catch(error){
   if(!(error instanceof ApiError)||![403,404,423,503].includes(error.status))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false);
   const retained=(await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id);if(retained?.witness!==denied[0].witness)throw changed();return {kind:'denied',error};
  }
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
