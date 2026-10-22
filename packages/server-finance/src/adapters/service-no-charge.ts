import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand,financialConstraintError} from '@erpx/server-workflow/public/commands';
import {fieldServiceFinanceSourceReadLocks} from '@erpx/server-field_service/public/finance-record-sources';
import {companyPurchaseReadLocks,financePackageEnforcementMode,checkPurchasedCapability} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {requireFinanceCurrentAuthority,withFinancePurchasedCommand} from '@erpx/server-finance/adapters/purchase-boundary';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
import {invoiceCommandRoutingSourceReadLocks,captureInvoiceCommandRoutingWitness} from '@erpx/server-workflow/public/invoice-command-routing-witness';
import {invoiceCommandRoutingCanAct} from '@erpx/server-workflow/public/invoice-command-routing-access';
import {serviceNoChargeSchema,type ServiceNoChargeControls,type ServiceNoChargeResult} from '@erpx/server-finance/domain/service-no-charge';

type Source={invoice_id:string;company_id:string;branch_id:string|null;department_id:string|null;invoice_version:number;invoice_status:string;service_order_id:string;billing_release_id:string;source_hash:string;independent:boolean};
const operation='finance.service-no-charge.confirm';
const identity=(actor:WorkContext)=>JSON.stringify([actor.userId,actor.companyId,actor.assignmentId,actor.role,actor.branchId,actor.departmentId,actor.authority??null,actor.iam??null]);
const hash=(value:string)=>createHash('sha256').update(value).digest('hex');
const unavailable=(error:unknown)=>!!error&&typeof error==='object'&&'code' in error&&error.code==='42P01';
async function source(database:SqlDatabase,actor:WorkContext,id:string){return database.prepare(`SELECT invoice_id,company_id,branch_id,department_id,invoice_version,invoice_status,service_order_id,billing_release_id,source_hash,NOT iam_same_person(?,created_by,company_id) AND NOT iam_same_person(?,released_by,company_id) independent FROM finance_service_no_charge_candidates WHERE invoice_id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)`).bind(actor.userId,actor.userId,id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<Source>();}
const contextKey=(actor:WorkContext,row:Source)=>hash(JSON.stringify([identity(actor),row.invoice_id,row.invoice_version,row.source_hash]));
/** Only this named constructor owns the complete source-leased zero-demand
 * decision, including disabled migration mode; it accepts no caller database. */
export async function confirmServiceNoCharge(actor:WorkContext,id:string,raw:unknown,key:string):Promise<ServiceNoChargeResult>{
 actor=requirePermission(actor,'finance.write');requirePermission(actor,'finance.approve');const input=serviceNoChargeSchema.parse(raw),owner=(await holdingResources(actor.holdingId)).write;
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'SERVICE_NO_CHARGE_TRANSACTION_REQUIRED','Use the original fresh Finance owner transaction.');
 try{return await owner.transaction(async database=>{
  const company=companyPurchaseReadLocks(database,actor.companyId);
  await company[0].run();await database.batch(fieldServiceFinanceSourceReadLocks(database));
  await database.prepare('LOCK TABLE finance_invoices,finance_service_no_charge_confirmations,command_receipts,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE').run();
  await database.prepare('LOCK TABLE finance_journals,finance_journal_lines,finance_allocations,finance_refund_obligations,finance_billing_corrections,finance_business_billing_runs,money_transaction_snapshots IN SHARE MODE').run();
  await database.batch(invoiceCommandRoutingSourceReadLocks(database));
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(company.slice(1));
  const currentIdentity=identity(actor),guard=`service-no-charge:${randomUUID()}`;await requireFinanceCurrentAuthority(database,actor,guard);
  const prior=await replayCommand<ServiceNoChargeResult>(database,actor,key,operation,id,input),mode=await financePackageEnforcementMode(),purpose=prior?'history' as const:'settle-existing' as const;
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'finance.service-invoice',purpose});
  const original=await source(database,actor,id);if(!original)throw new ApiError(409,'SERVICE_NO_CHARGE_SOURCE_CHANGED','Review the original fully covered Service billing source.');
  const routingWitness:{current:Awaited<ReturnType<typeof captureInvoiceCommandRoutingWitness>>|undefined}={current:undefined};
  const verify=async()=>{
   if(identity(actor)!==currentIdentity)throw new ApiError(409,'SERVICE_NO_CHARGE_CONTEXT_CHANGED','The selected Finance context changed.');
   await requireFinanceCurrentAuthority(database,actor,guard);await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'finance.service-invoice',purpose});
   await routingWitness.current?.assertCurrent();
   const after=await source(database,actor,id);if(!prior&&after&&!after.independent)throw new ApiError(403,'INDEPENDENT_APPROVER_REQUIRED','The original reviewer identity changed before confirmation.');if(!after||after.source_hash!==original.source_hash||after.invoice_status!=='no_charge_confirmed')throw new ApiError(409,'SERVICE_NO_CHARGE_SOURCE_CHANGED','The original zero-demand source changed before confirmation committed.');
   const terminal=await database.prepare('SELECT 1 FROM finance_service_no_charge_completed WHERE id=? AND company_id=? AND command_id=?').bind(id,actor.companyId,key).first();if(!terminal)throw new ApiError(409,'SERVICE_NO_CHARGE_PROOF_CHANGED','The exact independent no-charge confirmation is unavailable.');
  };
  if(prior){await flushFinanceDeferredEffects(database);await verify();return prior;}
  if(original.invoice_status!=='draft'||original.invoice_version!==input.expectedVersion||contextKey(actor,original)!==input.contextKey)throw new ApiError(409,'SERVICE_NO_CHARGE_CONTEXT_CHANGED','Refresh the captured no-charge review before confirming.');
  if(!original.independent)throw new ApiError(403,'INDEPENDENT_APPROVER_REQUIRED','Another Finance approver must confirm the original zero demand.');
  const confirmationId=`service-no-charge-${key}`,now=Date.now(),result:ServiceNoChargeResult={id,status:'no_charge_confirmed',version:input.expectedVersion+1,confirmationId};
  routingWitness.current=await captureInvoiceCommandRoutingWitness(database,actor,id,key,input,result.version);
  const execute=async()=>executeCommand(database,actor,key,operation,input,id,result.version,result,[
   assertion(database,key,'zero-source',`EXISTS(SELECT 1 FROM finance_service_no_charge_candidates WHERE invoice_id=? AND company_id=? AND invoice_status='draft' AND invoice_version=? AND source_hash=? AND NOT iam_same_person(?,created_by,company_id) AND NOT iam_same_person(?,released_by,company_id))`,[id,actor.companyId,input.expectedVersion,original.source_hash,actor.userId,actor.userId]),
   database.prepare(`INSERT INTO finance_service_no_charge_confirmations(id,company_id,invoice_id,service_order_id,billing_release_id,invoice_version_before,invoice_version_after,source_hash,status,independence_checked,confirmed_by,reason,command_id,confirmed_at) VALUES(?,?,?,?,?,?,?,?,'no_charge_confirmed',true,?,?,?,?)`).bind(confirmationId,actor.companyId,id,original.service_order_id,original.billing_release_id,input.expectedVersion,result.version,original.source_hash,actor.userId,input.reason,key,now),
   database.prepare("UPDATE finance_invoices SET status='no_charge_confirmed',approved_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='draft' AND version=?").bind(actor.userId,now,id,actor.companyId,input.expectedVersion),
  ]);
  const completed=await withFinancePurchasedCommand(actor,{key,operation,purpose:'settle-existing',capability:'finance.service-invoice'},execute,database);
  await flushFinanceDeferredEffects(database);await verify();return completed;
 });}catch(error){if(unavailable(error))throw new ApiError(503,'SERVICE_NO_CHARGE_SCHEMA_REQUIRED','The reviewed nonmonetary confirmation schema is not installed.');if(error&&typeof error==='object'&&'code' in error&&error.code==='23514'&&'message' in error&&typeof error.message==='string'&&/^SERVICE_NO_CHARGE_[A-Z_]+$/.test(error.message))throw new ApiError(409,error.message,'The original no-charge confirmation source or immutable decision changed.');throw financialConstraintError(error)??error;}
}
/** A minimal server seed. Missing old schema hides only this new operation. */
export async function serviceNoChargeControls(actor:WorkContext,id:string):Promise<ServiceNoChargeControls|null>{
 const database=(await holdingResources(actor.holdingId)).write;
 try{await requireFinanceCurrentAuthority(database,actor,`service-no-charge-controls:${randomUUID()}`);const row=await source(database,actor,id);if(!row)return null;
  const {readPurchasedAccesses}=await import('@erpx/server-commercial/public/purchase-command-boundary'),access=(await readPurchasedAccesses(database,await financePackageEnforcementMode(),actor.companyId,['finance.service-invoice']))['finance.service-invoice'];
  if(!access.history)return null;const {hasPermission}=await import('@erpx/feature-identity_access/lib/permissions');
  const canConfirm=row.invoice_status==='draft'&&row.independent&&hasPermission(actor,'finance.write')&&hasPermission(actor,'finance.approve')&&access.settleExisting;
  return {invoiceId:id,version:row.invoice_version,contextKey:contextKey(actor,row),canConfirm:canConfirm&&await invoiceCommandRoutingCanAct(database,actor,id),confirmed:row.invoice_status==='no_charge_confirmed'};
 }catch(error){if(unavailable(error))return null;throw error;}
}
