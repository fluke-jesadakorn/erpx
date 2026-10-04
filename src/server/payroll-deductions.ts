import "server-only";
import type {WorkContext} from "./authorization";
import type {SqlDatabase,SqlStatement} from "./runtime/types";
import {assertion} from "./commands";
import {ApiError} from "./http";

/** A payroll deduction consumes its approved installment at accrual, when AR is credited. */
export async function preparePayrollDeductionConsumption(write:SqlDatabase,actor:WorkContext,runId:string,journalId:string,receivableAccountId:string|undefined,key:string){
 const items=(await write.prepare("SELECT id,engagement_id,input_snapshot_json,result_json FROM hr_payroll_items WHERE run_id=? ORDER BY id").bind(runId).all<{id:string;engagement_id:string;input_snapshot_json:string;result_json:string}>()).results;
 const installments:Array<{id:string;version:number;amount:number;itemId:string;engagementId:string}>=[];
 for(const item of items){
  const snapshot=JSON.parse(item.input_snapshot_json) as {advanceRepayments?:Array<{id:string;version:number;amount_minor:number}>};
  const components=(JSON.parse(item.result_json) as {explanation?:{components?:Array<{code:string;kind:string;amountMinor:number}>}}).explanation?.components??[];
  const repayments=snapshot.advanceRepayments??[];
  for(const repayment of repayments){
   const matches=components.filter(component=>component.code===`payroll-advance:${repayment.id}`&&component.kind==='deduction');
   if(matches.length!==1||matches[0].amountMinor!==Number(repayment.amount_minor)||!Number.isSafeInteger(Number(repayment.amount_minor))||Number(repayment.amount_minor)<=0)
    throw new ApiError(409,"PAYROLL_DEDUCTION_SOURCE_CHANGED","The approved payroll deduction no longer matches its installment source.");
   installments.push({id:repayment.id,version:Number(repayment.version),amount:Number(repayment.amount_minor),itemId:item.id,engagementId:item.engagement_id});
  }
  if(components.some(component=>component.kind==='deduction'&&/^PAYROLL_ADVANCE_/u.test(component.code.toUpperCase().replaceAll(/[^A-Z0-9]+/gu,'_'))&&!repayments.some(repayment=>component.code===`payroll-advance:${repayment.id}`)))
   throw new ApiError(409,"PAYROLL_DEDUCTION_SOURCE_CHANGED","Every advance deduction requires its independently approved installment source.");
 }
 if(!installments.length)return {guards:[] as SqlStatement[],steps:[] as SqlStatement[]};
 if(!receivableAccountId||new Set(installments.map(row=>row.id)).size!==installments.length)
  throw new ApiError(409,"PAYROLL_DEDUCTION_SOURCE_CHANGED","Map the recovery receivable and consume each installment only once.");
 const guards:SqlStatement[]=[
  write.prepare(`SELECT a.id FROM hr_payroll_advances a WHERE a.id IN (SELECT advance_id FROM hr_payroll_advance_repayments WHERE id IN (${installments.map(()=>'?').join(',')})) ORDER BY a.id FOR UPDATE`).bind(...installments.map(row=>row.id)),
  write.prepare(`SELECT id FROM hr_payroll_advance_repayments WHERE id IN (${installments.map(()=>'?').join(',')}) ORDER BY id FOR UPDATE`).bind(...installments.map(row=>row.id)),
 ];
 const now=Date.now(),steps:SqlStatement[]=[];
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
 return {guards,steps};
}
