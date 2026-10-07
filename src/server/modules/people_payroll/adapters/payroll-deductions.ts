import "server-only";
import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "@/server/modules/runtime/public/runtime/types";
import {assertion} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {BusinessRuleError} from '../../shared';import {createPayrollDeductionApplication,type PayrollDeductionMutation,type PayrollDeductionItem} from '../application/payroll-deductions';export async function preparePayrollDeductionConsumption(write:SqlDatabase,actor:WorkContext,runId:string,journalId:string,receivableAccountId:string|undefined,key:string){try{return await createPayrollDeductionApplication({clock:{now:Date.now},items:async runId=>(await write.prepare('SELECT id,engagement_id,input_snapshot_json,result_json FROM hr_payroll_items WHERE run_id=? ORDER BY id').bind(runId).all<PayrollDeductionItem>()).results,stage:(m:PayrollDeductionMutation)=>{const{actor,runId,journalId,receivableAccountId,key,now,installments}=m; const guards:SqlStatement[]=[
  write.prepare(`SELECT a.id FROM hr_payroll_advances a WHERE a.id IN (SELECT advance_id FROM hr_payroll_advance_repayments WHERE id IN (${installments.map(()=>'?').join(',')})) ORDER BY a.id FOR UPDATE`).bind(...installments.map(row=>row.id)),
  write.prepare(`SELECT id FROM hr_payroll_advance_repayments WHERE id IN (${installments.map(()=>'?').join(',')}) ORDER BY id FOR UPDATE`).bind(...installments.map(row=>row.id)),
 ];
 const steps:SqlStatement[]=[];
 for(const [index,row] of installments.entries()){
  guards.push(assertion(write,key,`deduction-source-${index}`,`EXISTS(SELECT 1 FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id
    WHERE r.id=? AND r.company_id=? AND r.version=? AND r.amount_minor=? AND r.status IN ('open','reserved')
     AND (r.payroll_run_id IS NULL OR r.payroll_run_id=?) AND a.status='approved' AND a.engagement_id=?
     AND NOT EXISTS(SELECT 1 FROM hr_payroll_deduction_consumptions c WHERE c.repayment_id=r.id))`,[row.id,actor.companyId,row.version,row.amount,runId,row.engagementId]));
  steps.push(write.prepare(`INSERT INTO hr_payroll_deduction_consumptions(id,company_id,repayment_id,source_version,run_id,item_id,journal_id,receivable_account_id,amount_minor,command_id,created_at)
   VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(`payroll-deduction-${key}-${index}`,actor.companyId,row.id,row.version,runId,row.itemId,journalId,receivableAccountId,row.amount,key,now));
  steps.push(write.prepare("UPDATE hr_payroll_advance_repayments SET status='collected',payroll_run_id=?,version=version+1 WHERE id=? AND company_id=? AND version=? AND status IN ('open','reserved')").bind(runId,row.id,actor.companyId,row.version));
 }
 steps.push(write.prepare(`UPDATE hr_payroll_advances a SET status='closed',version=version+1,updated_at=? WHERE a.company_id=? AND a.status='approved'
  AND EXISTS(SELECT 1 FROM hr_payroll_deduction_consumptions c JOIN hr_payroll_advance_repayments r ON r.id=c.repayment_id WHERE c.run_id=? AND r.advance_id=a.id)
  AND NOT EXISTS(SELECT 1 FROM hr_payroll_advance_repayments r WHERE r.advance_id=a.id AND r.status!='collected')`).bind(now,actor.companyId,runId));
 return {guards,steps};}}).preparePayrollDeductionConsumption(actor,runId,journalId,receivableAccountId,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
