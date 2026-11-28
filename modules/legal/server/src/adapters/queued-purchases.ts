import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {legalPackageEnforcementMode} from '@erpx/commercial-server/public/purchase-command-boundary';
import {purchasedCapabilityPredicate} from '@erpx/commercial-server/public/purchased-capability-sql';
import {legalItemPurchasePredicate,withLegalPurchasedCommand,deferLegalOwnerCheck,inLegacyLegalOperationsUnit,requireCurrentLegalCommandAuthority} from '@erpx/legal-server/adapters/purchases';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/identity-access-server/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/pms-server/public/scheduled-feature-source-locks';
import {access,tables} from '@erpx/legal-server/adapters/legal/access';
import {legalCompanyCapabilityReady} from '@erpx/legal-server/adapters/legal/runtime-gate';
import {legalPurchaseClockExistsSql} from './legal-read-tail-r29-sources';
import {legalDisposedGuardSql} from './legal-read-tail-r39-sources';
import {legalCleanDocumentGuardHead} from './legal-read-tail-r49-sources';
import contract from './queued-purchases-sources.json' with {type:'json'};
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error('LEGAL_QUEUED_PURCHASES_CONTRACT_INVALID');
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
/** Actual owner item alias only; no caller label can select a capability. */
export async function legalQueuedPurchasePredicate(database:SqlDatabase,alias:string,kind:'ai'|'documents'|'invitations'|'feature'|'operations'){
 if(await legalPackageEnforcementMode()!=='enforced')return 'TRUE';
 const extra=kind==='ai'||kind==='documents'?['legal.documents']:kind==='invitations'?['legal.documents','legal.access']:kind==='feature'?['legal.obligations']:[];
 return legalPurchaseClockExistsSql('legal_queued_clock',`${legalItemPurchasePredicate(alias,'settle-existing','legal_queued_clock.now_ms')} ${extra.map(code=>`AND ${purchasedCapabilityPredicate(`${alias}.company_id`,code,'settle-existing','legal_queued_clock.now_ms')}`).join(' ')}`,'');
}
type JobTable='legal_ai_proposals'|'legal_processing_jobs';
type QueuedSource={item_id:string;company_id:string;actor_json:string;image:string;document_id:string|null;checksum:string|null};
async function job(database:SqlDatabase,table:JobTable,id:string){return database.prepare(fill(contract.jobSource,{table,documentColumns:table==='legal_processing_jobs'?'p.document_id,p.checksum':'NULL::text document_id,NULL::text checksum'})).bind(id).first<QueuedSource>();}
async function processingSourceCurrent(database:SqlDatabase,source:QueuedSource){
 if(!source.document_id)return;
 if(!await database.prepare(legalCleanDocumentGuardHead('d',`${legalDisposedGuardSql('x','d.id')} AND ${contract.processingSourceGuard}`)).bind(source.document_id,source.item_id,source.checksum).first())throw new ApiError(409,'LEGAL_JOB_CHANGED','The original queued document source changed.');
}
/** Provider inference stays outside this short owner transaction. Its publication
 * rechecks the queued source, current assignment, purchases and final effects. */
export async function withLegalQueuedCompletion(database:SqlDatabase,table:JobTable,id:string,run:(owner:SqlDatabase)=>Promise<void>){
 const mode=await legalPackageEnforcementMode();
 const queued=await job(database,table,id);if(!queued)throw new ApiError(404,'LEGAL_JOB_UNAVAILABLE','The original Legal job is unavailable.');
 const actor=JSON.parse(queued.actor_json) as WorkContext;if(actor.companyId!==queued.company_id)throw new ApiError(409,'LEGAL_JOB_CHANGED','The original queued Legal scope changed.');
 if(mode==='disabled')return inLegacyLegalOperationsUnit(database,queued.company_id,async owner=>{
  const before=await job(owner,table,id);if(!before||before.image!==queued.image||before.company_id!==queued.company_id)throw new ApiError(409,'LEGAL_JOB_CHANGED','The queued Legal source changed before completion.');
  const permission=table==='legal_ai_proposals'?'legal.ai':'legal.read',key=`legal-legacy-job:${randomUUID()}`;
  await requireCurrentLegalCommandAuthority(owner,actor,key,permission);const current=await requireQueuedCommandAuthority(owner,actor,permission);
  const source=JSON.stringify(await access(owner,current,before.item_id,permission));await processingSourceCurrent(owner,before);
  if(!await legalCompanyCapabilityReady(owner,current.companyId,table==='legal_ai_proposals'?'ai':'documents'))throw new ApiError(422,'LEGAL_INVALID','Legal processing readiness changed before completion.');
  await run(owner);const expected=await job(owner,table,id);
  if(!expected||expected.item_id!==before.item_id||expected.company_id!==before.company_id||expected.actor_json!==before.actor_json||expected.document_id!==before.document_id||expected.checksum!==before.checksum)throw new ApiError(409,'LEGAL_JOB_CHANGED','The completed queued Legal binding changed.');
  deferLegalOwnerCheck(owner,current.companyId,async()=>{
   await requireCurrentLegalCommandAuthority(owner,current,key,permission);await requireQueuedCommandAuthority(owner,current,permission);
   if(JSON.stringify(await access(owner,current,before.item_id,permission))!==source)throw new ApiError(409,'LEGAL_SOURCE_CHANGED','The queued Legal source changed during completion.');
   await processingSourceCurrent(owner,before);const final=await job(owner,table,id);if(!final||expected.image!==final.image||expected.company_id!==final.company_id)throw new ApiError(409,'LEGAL_JOB_CHANGED','The completed queued Legal source changed during deferred effects.');
   if(!await legalCompanyCapabilityReady(owner,current.companyId,table==='legal_ai_proposals'?'ai':'documents'))throw new ApiError(422,'LEGAL_INVALID','Legal processing readiness changed during completion.');
  });
 },async owner=>{
  await owner.prepare(`LOCK TABLE ${table},legal_settings,legal_disposals,legal_destructions,${Object.values(tables).join(',')} IN SHARE ROW EXCLUSIVE MODE`).run();
  await lockQueuedCommandAuthority(owner);await lockScheduledFeatureSources(owner);
 });
 return withLegalPurchasedCommand(database,actor,{key:`legal-job:${randomUUID()}`,id:queued.item_id,operation:'legal.job.complete',purpose:'settle-existing',permission:table==='legal_ai_proposals'?'legal.ai':'legal.read',additional:['legal.documents']},async owner=>{
  const before=await job(owner,table,id);if(!before||before.image!==queued.image)throw new ApiError(409,'LEGAL_JOB_CHANGED','The queued Legal source changed before completion.');await access(owner,actor,queued.item_id,table==='legal_ai_proposals'?'legal.ai':'legal.read');
  await run(owner);const expected=await job(owner,table,id);deferLegalOwnerCheck(owner,actor.companyId,async()=>{const current=await job(owner,table,id);if(!expected||!current||expected.image!==current.image)throw new ApiError(409,'LEGAL_JOB_CHANGED','The completed Legal job changed during deferred effects.');await access(owner,actor,queued.item_id,table==='legal_ai_proposals'?'legal.ai':'legal.read');});
 });
}
