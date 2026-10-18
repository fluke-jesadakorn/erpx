import {serviceInitialStockJournalMarked} from './service-initial-stock-journal-sources';
import {posInitialStockJournalMarked} from './pos-initial-stock-journal-sources';
import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '../../identity_access/public/private-file-denial-authority';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {companyPurchaseReadLocks} from '../../commercial/public/purchase-command-boundary';
import {flushFinanceDeferredEffects} from './finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,attachmentReadChanged,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '../../files/public/private-attachment-reads';
import {attachmentDeniedReadAudits} from '../../files/public/attachment-denied-read-audits';
import {isFinanceAttachmentReadTarget,financeAttachmentReadSourceLocks,serviceInitialStockJournalReadSourceLocks,manufacturingInitialStockJournalReadSourceLocks,financeAttachmentReadSourceImage} from './attachment-read-sources';
import {manufacturingInitialStockJournalMarked} from './manufacturing-initial-stock-journal-sources';
const identity=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.userId,actor.email,actor.assignmentId,actor.role,actor.branchId,actor.departmentId,actor.assignmentScope,actor.scope,actor.modules,actor.authority??null,actor.iam??null]);
/** Exact native Finance owners keep their original complete File policy and
 * actual included-owner purchase decisions. This constructor adds no Finance
 * role or purchased capability and cannot inherit an arbitrary active parent. */
export async function withFinanceAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(!isFinanceAttachmentReadTarget(selected.targetType))throw attachmentReadChanged();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'FINANCE_FILE_READ_TRANSACTION_REQUIRED','Private Finance evidence requires its original complete fresh read transaction.');
 const target=selected.targetType,context=identity(actor);
 // Raw persisted shape selects a fresh owner lease path. It grants neither
 // authority nor bytes, and must remain identical after locks and before return.
 const serviceSelected=target==='finance_journal'&&await serviceInitialStockJournalMarked(owner,actor.companyId,selected.targetId);
 const posSelected=target==='finance_journal'&&await posInitialStockJournalMarked(owner,actor.companyId,selected.targetId);
 const manufacturingSelected=target==='finance_journal'&&await manufacturingInitialStockJournalMarked(owner,actor.companyId,selected.targetId);
 type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome=await owner.transaction(async(database):Promise<Outcome>=>{
  // Strong package/source fence excludes normal enabled writers before their
  // Company advisory lock. No SELECT or owner snapshot precedes these leases.
  await lockPackageSettlementSources(database);
  await database.batch([...(serviceSelected?serviceInitialStockJournalReadSourceLocks(database,actor.companyId):(manufacturingSelected||posSelected)?manufacturingInitialStockJournalReadSourceLocks(database,actor.companyId):financeAttachmentReadSourceLocks(database,actor.companyId)),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  const assertOwnerSelection=async()=>{const actual=target==='finance_journal'&&await manufacturingInitialStockJournalMarked(database,actor.companyId,selected.targetId),actualService=target==='finance_journal'&&await serviceInitialStockJournalMarked(database,actor.companyId,selected.targetId),actualPos=target==='finance_journal'&&await posInitialStockJournalMarked(database,actor.companyId,selected.targetId);if(actual!==manufacturingSelected||actualPos!==posSelected||actualService!==serviceSelected)throw attachmentReadChanged();};
  await assertOwnerSelection();
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw attachmentReadChanged();
  const before=await financeAttachmentReadSourceImage(database,actor.companyId,target,source.targetId),current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
  const invalidAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor);
  const check=async(authorizeSource:boolean)=>{
   await assertOwnerSelection();
   if(identity(actor)!==context)throw attachmentReadChanged();if(authorizeSource||initiallyCurrent)await requirePrivateAttachmentReaderCurrent(database,actor);else if(await privateFileDenialAuthorityWitness(database,actor)!==invalidAuthority)throw attachmentReadChanged();if(authorizeSource)await authorize(database,source);
   const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);
   if(after?.witness!==source.witness||await financeAttachmentReadSourceImage(database,actor.companyId,target,source.targetId)!==before)throw attachmentReadChanged();
  };
  const checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));let result:Result;
  try{result=await read(database);}catch(error){
   if(!checkpoint||!(error instanceof ApiError)||!([403,404].includes(error.status)||error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE'||error.status===503&&error.code==='FILE_UNAVAILABLE'))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false);
   const retained=(await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id);if(retained?.witness!==denied[0].witness)throw attachmentReadChanged();
   return {kind:'denied',error};
  }
  if(!before)throw attachmentReadChanged();const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);
  return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });
 if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
