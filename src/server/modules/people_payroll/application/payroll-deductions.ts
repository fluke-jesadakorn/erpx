import {BusinessRuleError,type Clock} from '../../shared';import type {WorkContext} from '../../identity_access/public/authorization';export type PayrollDeductionItem={id:string;engagement_id:string;input_snapshot_json:string;result_json:string};export type PayrollDeductionMutation={actor:WorkContext;runId:string;journalId:string;receivableAccountId:string;key:string;now:number;installments:Array<{id:string;version:number;amount:number;itemId:string;engagementId:string}>};export interface PayrollDeductionPorts<Effect>{clock:Clock;items(runId:string):Promise<PayrollDeductionItem[]>;stage(mutation:PayrollDeductionMutation):{guards:Effect[];steps:Effect[]};}export function createPayrollDeductionApplication<Effect>(ports:PayrollDeductionPorts<Effect>){async function preparePayrollDeductionConsumption(actor:WorkContext,runId:string,journalId:string,receivableAccountId:string|undefined,key:string){
 const items=await ports.items(runId);
 const installments:Array<{id:string;version:number;amount:number;itemId:string;engagementId:string}>=[];
 for(const item of items){
  const snapshot=JSON.parse(item.input_snapshot_json) as {advanceRepayments?:Array<{id:string;version:number;amount_minor:number}>};
  const components=(JSON.parse(item.result_json) as {explanation?:{components?:Array<{code:string;kind:string;amountMinor:number}>}}).explanation?.components??[];
  const repayments=snapshot.advanceRepayments??[];
  for(const repayment of repayments){
   const matches=components.filter(component=>component.code===`payroll-advance:${repayment.id}`&&component.kind==='deduction');
   if(matches.length!==1||matches[0].amountMinor!==Number(repayment.amount_minor)||!Number.isSafeInteger(Number(repayment.amount_minor))||Number(repayment.amount_minor)<=0)
    throw new BusinessRuleError(409,"PAYROLL_DEDUCTION_SOURCE_CHANGED","The approved payroll deduction no longer matches its installment source.");
   installments.push({id:repayment.id,version:Number(repayment.version),amount:Number(repayment.amount_minor),itemId:item.id,engagementId:item.engagement_id});
  }
  if(components.some(component=>component.kind==='deduction'&&/^PAYROLL_ADVANCE_/u.test(component.code.toUpperCase().replaceAll(/[^A-Z0-9]+/gu,'_'))&&!repayments.some(repayment=>component.code===`payroll-advance:${repayment.id}`)))
   throw new BusinessRuleError(409,"PAYROLL_DEDUCTION_SOURCE_CHANGED","Every advance deduction requires its independently approved installment source.");
 }
 if(!installments.length)return {guards:[] as Effect[],steps:[] as Effect[]};if(!receivableAccountId||new Set(installments.map(row=>row.id)).size!==installments.length)throw new BusinessRuleError(409,'PAYROLL_DEDUCTION_SOURCE_CHANGED','Map the recovery receivable and consume each installment only once.');return ports.stage({actor,runId,journalId,receivableAccountId,key,now:ports.clock.now(),installments});
}return {preparePayrollDeductionConsumption};}
