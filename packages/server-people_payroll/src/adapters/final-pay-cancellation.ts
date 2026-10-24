import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {payrollCancellationGrant} from '@erpx/server-identity_access/public/payroll-cancellation-grant';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {companyPurchaseReadLocks,peoplePackageEnforcementMode,readPurchasedAccess,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {payrollFinalPayCancellationFinanceSourceLocks} from '@erpx/server-finance/public/payroll-final-pay-cancellation-sources';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createFinalPayCancellationApplication,type FinalPayCancellationMutation} from '@erpx/server-people_payroll/application/final-pay-cancellation';
import {recordAttachmentVersionSourceReadLocks} from '@erpx/server-files/public/record-version-receipts';
import {finalPayCancellationTerminal} from '@erpx/server-people_payroll/adapters/final-pay-cancellation-terminal-sources';
import {readFinalPayCancellationSource,finalPayCancellationSchemaAvailable} from '@erpx/server-people_payroll/adapters/final-pay-cancellation-sources';
import type {FinalPayCancellationInput,FinalPayCancellationResult,FinalPayCancellationSource} from '@erpx/server-people_payroll/domain/final-pay-cancellation';
const operation='people.payroll.final-pay.cancel';
const changed=()=>new ApiError(409,'FINAL_PAY_CANCELLATION_SOURCE_CHANGED','The original final-pay preparation, cancelled case/task or money evidence changed. Reload its current source.');
const authorityImage=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.userId,actor.assignmentId,actor.role,actor.branchId,actor.departmentId,actor.scope,actor.authority,actor.iam]);
async function currentAuthority(database:SqlDatabase,actor:WorkContext){requirePermission(actor,'payroll.prepare');if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_PAYROLL_SCOPE_REQUIRED','Use a current Company Payroll preparation assignment.');const current=await requireQueuedCommandAuthority(database,actor,'payroll.prepare');if(!await payrollCancellationGrant(database,actor))throw new ApiError(403,'PAYROLL_CANCELLATION_FORBIDDEN','The current Company role does not permit Payroll preparation.');return current;}
/** Package first, receipt before all disabled receipt-first writers; every
 * source/Finance/authority fence is acquired before the Company advisory read. */
async function leases(database:SqlDatabase,companyId:string){
 await lockPackageSettlementSources(database);
 await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();
 await database.prepare('LOCK TABLE hr_payroll_runs,hr_payroll_final_pay_preparations,hr_payroll_final_pay_events,people_lifecycle_cases,people_preparation_tasks,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE').run();
 await database.prepare('LOCK TABLE hr_engagements,hr_payroll_items,hr_payroll_postings,hr_issued_payslips,hr_payment_batches,hr_payroll_deduction_consumptions,hr_payroll_corrections IN SHARE MODE').run();
 await database.batch([...payrollFinalPayCancellationFinanceSourceLocks(database),...recordAttachmentVersionSourceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await database.batch(companyPurchaseReadLocks(database,companyId).slice(1));
}
function unchangedFields(before:string,after:string,fields:string[]){const a=JSON.parse(before) as Record<string,unknown>,b=JSON.parse(after) as Record<string,unknown>;for(const field of fields){delete a[field];delete b[field];}return JSON.stringify(a)===JSON.stringify(b);}
async function assertOwnTransition(database:SqlDatabase,actor:WorkContext,m:FinalPayCancellationMutation){
 const after=await readFinalPayCancellationSource(database,actor,m.source.runId,m.result.cancellationId);
 if(!after||after.status!=='cancelled'||after.runStatus!=='cancelled'||after.version!==m.result.version||after.runVersion!==m.result.runVersion||after.taskImage!==m.source.taskImage||after.caseImage!==m.source.caseImage||after.engagementImage!==m.source.engagementImage||after.childrenImage!==m.source.childrenImage||after.eventsImage!==m.source.eventsImage||!after.noMoneyEffects||!unchangedFields(m.source.preparationImage,after.preparationImage,['status','version','updated_at'])||!unchangedFields(m.source.runImage,after.runImage,['status','version','updated_at']))throw changed();
 if(!await database.prepare('SELECT hr_final_pay_cancellation_valid(?) valid').bind(m.source.id).first<boolean>('valid'))throw changed();
 const full=await readFinalPayCancellationSource(database,actor,m.source.runId);if(!full)throw changed();return full;
}
async function owned<Result>(actor:WorkContext,runId:string,key:string|undefined,run:(database:SqlDatabase,schemaAvailable:boolean,source:()=>Promise<FinalPayCancellationSource|null>,post:(image:string)=>void)=>Promise<Result>){
 requirePermission(actor,'payroll.prepare');if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_PAYROLL_SCOPE_REQUIRED','Use a current Company Payroll preparation assignment.');
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'PAYROLL_TRANSACTION_REQUIRED','Final-pay cancellation requires its owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  await leases(database,actor.companyId);const authorized=await currentAuthority(database,actor),schemaAvailable=await finalPayCancellationSchemaAvailable(database),mode=await peoplePackageEnforcementMode();
  const prior=key?await database.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(key).first():null,purchase={companyId:actor.companyId,capability:'people.payroll',purpose:key&&!prior?'settle-existing' as const:'history' as const},fence=`final-pay-cancel:${randomUUID()}`;
  if(mode!=='disabled')await checkPurchasedCapability(database,mode,purchase);
  if(key&&!schemaAvailable)throw new ApiError(503,'FINAL_PAY_CANCELLATION_SCHEMA_REQUIRED','The reviewed final-pay cancellation schema is required.');
  const boundary=key?purchaseCommandBoundary(database,mode,fence,purchase):null;if(boundary)await database.batch(boundary.before);
  let captured:FinalPayCancellationSource|null|undefined,expected:string|undefined;
  const source=async()=>{if(captured===undefined){captured=await readFinalPayCancellationSource(database,actor,runId);expected=captured?.image;}return captured;};
  const result=await run(database,schemaAvailable,source,image=>{expected=image;});
  if(boundary){const observed=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(fence)]);boundary.observe?.(observed);}
  if(key)await flushFinanceDeferredEffects(database);
  const final=await currentAuthority(database,actor);if(authorityImage(authorized)!==authorityImage(final))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original Payroll preparation authority changed.');
  if(captured!==undefined&&(await readFinalPayCancellationSource(database,actor,runId))?.image!==expected)throw changed();
  if(mode!=='disabled')await checkPurchasedCapability(database,mode,purchase);return result;
 });}catch(error){if(String(error).includes('PAYROLL_FINAL_PAY_CANCELLATION_SOURCE_CHANGED'))throw changed();if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export function getFinalPayCancellation(actor:WorkContext,runId:string){return owned(actor,runId,undefined,async(database,schemaAvailable,source)=>{const view=await createFinalPayCancellationApplication({clock:{now:Date.now},authorize:requirePermission,currentAuthority:async a=>{await currentAuthority(database,a);},schemaAvailable:async()=>schemaAvailable,source:async()=>source(),replay:async()=>null,commit:async()=>{throw Error('FINAL_PAY_READ_ONLY');}}).getFinalPayCancellation(actor,runId);if(schemaAvailable&&view.status==='cancelled'&&(!await finalPayCancellationTerminal(database,actor.companyId,view.preparationId)))return {...view,canCancel:false,unavailableReason:'source-unavailable' as const};const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return view;const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'people.payroll'});return {...view,canCancel:view.canCancel&&access.settleExisting};});}
export function cancelFinalPayPreparation(actor:WorkContext,runId:string,raw:unknown,key:string){return owned(actor,runId,key,async(database,schemaAvailable,source,post)=>createFinalPayCancellationApplication({clock:{now:Date.now},authorize:requirePermission,currentAuthority:async a=>{await currentAuthority(database,a);},schemaAvailable:async()=>schemaAvailable,source:async()=>source(),replay:async(a,k,id,input)=>{const prior=await replayCommand<FinalPayCancellationResult>(database,a,k,operation,id,input);if(prior&&(!await finalPayCancellationTerminal(database,actor.companyId,id)||!await database.prepare('SELECT hr_final_pay_cancellation_valid(?) valid').bind(id).first<boolean>('valid')||prior.id!==id||prior.runId!==runId||prior.cancellationId!==`final-pay-cancel-event-${k}`))throw changed();return prior;},commit:async m=>{
 const s=m.source,evidence=JSON.stringify({preparation:JSON.parse(s.preparationImage),run:JSON.parse(s.runImage),task:JSON.parse(s.taskImage),case:JSON.parse(s.caseImage),reason:m.input.reason});
 const result=await executeCommand(database,actor,key,operation,m.input,s.id,m.result.version,m.result,[
  assertion(database,key,'final-pay-cancellation-source',`EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations p JOIN hr_payroll_runs r ON r.id=p.run_id AND r.company_id=p.company_id JOIN people_preparation_tasks t ON t.id=p.task_id AND t.company_id=p.company_id JOIN people_lifecycle_cases c ON c.id=p.case_id AND c.company_id=p.company_id WHERE p.id=? AND p.company_id=? AND to_jsonb(p)=?::jsonb AND to_jsonb(r)=?::jsonb AND to_jsonb(t)=?::jsonb AND to_jsonb(c)=?::jsonb)`,[s.id,actor.companyId,s.preparationImage,s.runImage,s.taskImage,s.caseImage]),
  database.prepare("INSERT INTO hr_payroll_final_pay_events(id,preparation_id,task_id,run_id,company_id,action,actor_id,task_version,evidence_json,created_at) VALUES(?,?,?,?,?,'cancelled',?,?,?,?)").bind(m.result.cancellationId,s.id,s.taskId,s.runId,actor.companyId,actor.userId,s.taskVersion,evidence,m.now),
  database.prepare("UPDATE hr_payroll_runs SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(m.now,s.runId,actor.companyId,s.runVersion),
  database.prepare("UPDATE hr_payroll_final_pay_preparations SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(m.now,s.id,actor.companyId,s.version),
 ]);
 post((await assertOwnTransition(database,actor,m)).image);return result;
}}).cancelFinalPayPreparation(actor,runId,raw,key));}
export type {FinalPayCancellationInput};
