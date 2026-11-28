import "server-only";
import { ppRd35PayrollItemsByRun } from './pp-read-tail-r35-sources';
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase,SqlStatement} from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import { payrollDeductions } from '@erpx/collaboration-server/adapters/native-payroll-deductions';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';import {createPayrollDeductionApplication,type PayrollDeductionMutation,type PayrollDeductionItem} from '@erpx/people-payroll-server/application/payroll-deductions';export async function preparePayrollDeductionConsumption(write:SqlDatabase,actor:WorkContext,runId:string,journalId:string,receivableAccountId:string|undefined,key:string){try{return await createPayrollDeductionApplication({clock:{now:Date.now},items:async runId=>(await write.prepare(ppRd35PayrollItemsByRun('id,engagement_id,input_snapshot_json,result_json', ' ORDER BY id')).bind(runId).all<PayrollDeductionItem>()).results,stage:async (m:PayrollDeductionMutation)=>{const{actor,runId,journalId,receivableAccountId,key,now,installments}=m; const guards:SqlStatement[]=[
  write.prepare(`SELECT a.id FROM hr_payroll_advances a WHERE a.id IN (SELECT advance_id FROM hr_payroll_advance_repayments WHERE id IN (${installments.map(()=>'?').join(',')})) ORDER BY a.id FOR UPDATE`).bind(...installments.map(row=>row.id)),
  write.prepare(`SELECT id FROM hr_payroll_advance_repayments WHERE id IN (${installments.map(()=>'?').join(',')}) ORDER BY id FOR UPDATE`).bind(...installments.map(row=>row.id)),
 ];
 const steps:SqlStatement[]=[];
 for(const [index,row] of installments.entries()){
  guards.push(assertion(write,key,`deduction-source-${index}`,`EXISTS(SELECT 1 FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id
    WHERE r.id=? AND r.company_id=? AND r.version=? AND r.amount_minor=? AND r.status IN ('open','reserved')
     AND (r.payroll_run_id IS NULL OR r.payroll_run_id=?) AND a.status='approved' AND a.engagement_id=?
     AND NOT EXISTS(SELECT 1 FROM hr_payroll_deduction_consumptions c WHERE c.repayment_id=r.id))`,[row.id,actor.companyId,row.version,row.amount,runId,row.engagementId]));
  steps.push(await payrollDeductions(write, actor.companyId).consumption({ id: `payroll-deduction-${key}-${index}`, companyId: actor.companyId, repaymentId: row.id, sourceVersion: row.version, runId, itemId: row.itemId, journalId, receivableAccountId, amount: row.amount, key, now }));
  steps.push(await payrollDeductions(write, actor.companyId).collect({ runId, repaymentId: row.id, companyId: actor.companyId, version: row.version }));
 }
 steps.push(await payrollDeductions(write, actor.companyId).close({ now, companyId: actor.companyId, runId }));
 return {guards,steps};}}).preparePayrollDeductionConsumption(actor,runId,journalId,receivableAccountId,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
