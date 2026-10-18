import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '../../identity_access/public/private-file-denial-authority';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {attachmentDeniedReadAudits} from '../../files/public/attachment-denied-read-audits';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {attachmentPrivateReadSourceLocks,attachmentReadSource,assertAttachmentReadRoute,attachmentReadChanged,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '../../files/public/private-attachment-reads';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {requireLegalAttachmentReadPreparation,deferLegalAttachmentReadCheck} from '../public/attachment-read-parent';
import {inLegalCanonicalDocumentReadUnit,legalSourceWitness} from './purchases';

const identity=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.userId,actor.email,actor.assignmentId,actor.role,actor.branchId,actor.departmentId,actor.scope,actor.assignmentScope,actor.modules,actor.authority??null,actor.iam??null]);
const parents=new WeakMap<SqlDatabase,string>();
const deniedReadParents=new WeakMap<SqlDatabase,boolean>();
/** Only the standalone reader records this after observing the one actual
 * original Files denial. Complete mutation/read parents cannot inherit it. */
export function legalCanonicalDenialReadAuthority(database:SqlDatabase,actor:WorkContext){return parents.get(database)===identity(actor)&&deniedReadParents.has(database)?deniedReadParents.get(database)?'current':'unchanged-invalid':null;}
export async function prepareLegalCanonicalAttachmentParentRead(database:SqlDatabase,actor:WorkContext){
 requireLegalAttachmentReadPreparation(database,actor.companyId);
 await database.prepare('LOCK TABLE legal_settings,legal_holds,legal_disposals,legal_destructions,legal_external_uploads,legal_requests,legal_matters,legal_contracts,legal_obligations,legal_compliance,legal_licenses,legal_corporate,legal_ip_assets,legal_privacy,legal_counsel,legal_templates,legal_rules,legal_executed_copies,legal_approvals,legal_approval_routes,legal_authorities,legal_retention_assignments,legal_retention_policies IN SHARE ROW EXCLUSIVE MODE').run();
 parents.set(database,identity(actor));
}
/** Finish every business owner lease before acquiring shared File/IAM leases. */
export async function prepareLegalCanonicalAttachmentSharedSources(database:SqlDatabase){await attachmentPrivateReadSourceLocks(database).run();await database.batch(privateFileDenialAuthoritySourceLocks(database));await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);}
async function image(database:SqlDatabase,actor:WorkContext,id:string){const original=await legalSourceWitness(database,actor,id);if(!original)return null;return database.prepare(`SELECT jsonb_build_object('owner',?::jsonb,'settings',(SELECT to_jsonb(s) FROM legal_settings s WHERE s.company_id=i.company_id),'holds',(SELECT jsonb_agg(to_jsonb(h) ORDER BY h.id) FROM legal_holds h WHERE h.item_id=i.id),'disposals',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.document_id) FROM legal_disposals d JOIN legal_documents doc ON doc.id=d.document_id WHERE doc.item_id=i.id),'destructions',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM legal_destructions d WHERE d.item_id=i.id),'uploads',(SELECT jsonb_agg(to_jsonb(u) ORDER BY u.id) FROM legal_external_uploads u JOIN legal_shares share ON share.id=u.share_id WHERE share.item_id=i.id))::text image FROM legal_items i WHERE i.id=? AND i.company_id=?`).bind(original,id,actor.companyId).first<string>('image');}
/** Stored text and legacy evidence can bypass FileStore. Their actual Legal
 * source still belongs to the same final whole-parent source check. */
export async function deferLegalCanonicalDocumentSourceRead(database:SqlDatabase,actor:WorkContext,itemId:string){
 if(parents.get(database)!==identity(actor))throw new ApiError(503,'LEGAL_CANONICAL_READ_PARENT_REQUIRED','Canonical Legal bytes require their original complete read parent.');
 const before=await image(database,actor,itemId);if(!before)throw new ApiError(404,'LEGAL_NOT_FOUND','The original Legal source is unavailable.');
 deferLegalAttachmentReadCheck(database,actor.companyId,async()=>{if(parents.get(database)!==identity(actor)||await image(database,actor,itemId)!==before)throw attachmentReadChanged();});
}
export async function withLegalCanonicalAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(selected.targetType!=='legal_item')throw attachmentReadChanged();
 type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const execute=async(database:SqlDatabase,standalone=false):Promise<Outcome>=>{
  if(parents.get(database)!==identity(actor))throw new ApiError(503,'LEGAL_CANONICAL_READ_PARENT_REQUIRED','Canonical Legal bytes require their original complete read parent.');
  const before=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,before);if(!before)throw attachmentReadChanged();const original=await image(database,actor,before.targetId);if(!original)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original Legal document is unavailable.');
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=standalone&&!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
  const invalidAuthority=standalone&&!initiallyCurrent?await privateFileDenialAuthorityWitness(database,actor):null;
  const checkpoint=standalone?new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id)):null;let result:Result;
  try{result=await read(database);}catch(error){
   if(!checkpoint||!(error instanceof ApiError)||!([403,404].includes(error.status)||error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE'||error.status===503&&error.code==='FILE_UNAVAILABLE'))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;
   deniedReadParents.set(database,initiallyCurrent);
   deferLegalAttachmentReadCheck(database,actor.companyId,async()=>{
    if(parents.get(database)!==identity(actor))throw attachmentReadChanged();if(initiallyCurrent)await requirePrivateAttachmentReaderCurrent(database,actor);else if(await privateFileDenialAuthorityWitness(database,actor)!==invalidAuthority)throw attachmentReadChanged();
    const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(before,after);if(after?.witness!==before.witness||await image(database,actor,before.targetId)!==original)throw attachmentReadChanged();
    const retained=(await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id);if(retained?.witness!==denied[0].witness)throw attachmentReadChanged();
   });return {kind:'denied',error};
  }
  const bytes=await result.object.arrayBuffer();
  deferLegalAttachmentReadCheck(database,actor.companyId,async()=>{if(parents.get(database)!==identity(actor))throw attachmentReadChanged();await requirePrivateAttachmentReaderCurrent(database,actor);await authorize(database,before);const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(before,after);if(after?.witness!==before.witness||await image(database,actor,before.targetId)!==original)throw attachmentReadChanged();});
  return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 };
 const unwrap=(outcome:Outcome)=>{if(outcome.kind==='denied')throw outcome.error;return outcome.result;};
 if(parents.get(owner)===identity(actor))return unwrap(await execute(owner));
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'LEGAL_CANONICAL_READ_PARENT_REQUIRED','An arbitrary active snapshot cannot inherit canonical Legal bytes.');
 return unwrap(await inLegalCanonicalDocumentReadUnit(owner,actor,'legal.read',database=>execute(database,true)));
}
