import {z} from "zod";
import type {WorkItem} from "@/lib/work-item";
import {decimalToMinor,minorToDecimal} from "@/lib/finance-money";
import {BusinessRuleError} from "../../shared";
export type SupplierPaymentRow=Record<string,string|number|null>;
const version = z.number().int().positive();
const reason = z.string().trim().min(3).max(1000);
export const supplierBatchSchema = z.object({
 bankAccountId: z.string().min(1), plannedDate: z.iso.date(),
 items: z.array(z.object({invoiceId:z.string().min(1),expectedVersion:version,amount:z.string().regex(/^\d+(\.\d{1,2})?$/),beneficiaryName:z.string().trim().min(2).max(160),beneficiaryAccount:z.string().trim().min(3).max(160)})).min(1).max(50),
});
export const supplierDecisionSchema = z.object({expectedVersion:version,reason});
export const supplierExportSchema = z.object({expectedVersion:version});
export const supplierResultSchema = z.object({expectedVersion:version,outcome:z.enum(['unknown','failed']),evidence:reason});
export const supplierConfirmSchema = z.object({expectedVersion:version,bankTransactionId:z.string().min(1),bankVersion:version,periodId:z.string().min(1),bankReference:z.string().trim().min(2).max(160),evidence:reason,attachmentSetIds:z.array(z.string().min(1)).max(20).default([])});

export type BatchInput=z.infer<typeof supplierBatchSchema>;export type BatchDecision=z.infer<typeof supplierDecisionSchema>;export type BatchExport=z.infer<typeof supplierExportSchema>;export type InstructionResult=z.infer<typeof supplierResultSchema>;export type InstructionConfirmation=z.infer<typeof supplierConfirmSchema>;
export type PreparedBatchItem={index:number;item:BatchInput['items'][number];amount:number};
export type PaymentEvidenceSet={id:string;purpose_key:string;component_kind:string;component_ref:string|null};
export function assertCompanyTreasury(branchId:string|null,departmentId:string|null){if(branchId||departmentId)throw new BusinessRuleError(403,'COMPANY_TREASURY_REQUIRED','Use a Company finance assignment for supplier bank batches.');}
export function assertIndependentPayer(batch:SupplierPaymentRow,samePerson:(id:unknown)=>boolean){if(samePerson(batch.created_by)||samePerson(batch.approved_by))throw new BusinessRuleError(403,'INDEPENDENT_PAYER_REQUIRED','A third person must export and confirm this approved batch.');}
export function assertSupplierBatchUnique(input:BatchInput){if(new Set(input.items.map(x=>x.invoiceId)).size!==input.items.length)throw new BusinessRuleError(422,'DUPLICATE_INVOICE','Select each invoice once.');}
export function prepareSupplierBatchItems(input:BatchInput){return input.items.map((item,index)=>{const amount=decimalToMinor(item.amount);if(amount<=0)throw new BusinessRuleError(422,'INVALID_AMOUNT','Use a positive payment amount.');return {index,item,amount};});}
export function assertSupplierBank(bank:SupplierPaymentRow|null){if(!bank||bank.currency!==bank.base_currency)throw new BusinessRuleError(422,'BANK_ACCOUNT_INVALID','Select an active bank ledger in the functional currency.');return bank;}
export function assertSupplierBatchDecision(batch:SupplierPaymentRow,decision:'approve'|'cancel',samePerson:(id:unknown)=>boolean,canApprove:boolean){if(decision==='approve'&&samePerson(batch.created_by))throw new BusinessRuleError(403,'INDEPENDENT_APPROVER_REQUIRED','A different person must review the source, amount and beneficiary.');if(decision==='cancel'&&!samePerson(batch.created_by)&&!canApprove)throw new BusinessRuleError(403,'CANCEL_FORBIDDEN','The preparer or Finance approver must cancel this batch.');if(batch.status!=='draft'&&(decision==='approve'||batch.status!=='approved'))throw new BusinessRuleError(409,'BATCH_NOT_EDITABLE','Exported batches require individual bank-result review.');}
export function assertPendingInstruction(item:SupplierPaymentRow,confirm:boolean){if(!['exported','unknown'].includes(String(item.status)))throw new BusinessRuleError(409,'INSTRUCTION_NOT_PENDING',confirm?'Confirm an exported instruction.':'Only an unresolved exported instruction can receive a bank result.');}
export function assertPaymentBankEvidence(bank:SupplierPaymentRow|null,batch:SupplierPaymentRow,amount:number){if(!bank||bank.currency!==batch.currency||bank.bank_account!==batch.bank_account_id||Number(bank.amount_minor)!==-amount||String(bank.transaction_date)<String(batch.planned_date))throw new BusinessRuleError(422,'BANK_EVIDENCE_MISMATCH','Match the approved bank account, currency, date and exact outgoing amount.');return bank;}
export function assertSupplierPaymentEvidence(sets:PaymentEvidenceSet[],ids:string[],id:string){if(sets.length!==new Set(ids).size||sets.some(set=>set.purpose_key!=='payment_proof'||set.component_kind!=='action'||set.component_ref!==`supplier-instruction:${id}`))throw new BusinessRuleError(422,'SUPPLIER_ATTACHMENT_PURPOSE','Choose only payment proof prepared for this supplier instruction.');}
const csvCell=(value:unknown)=>`"${String(value??'').replace(/^[=+@\-\t\r]/,"'$&").replaceAll('"','""')}"`;
export function supplierExportContent(batch:SupplierPaymentRow,items:SupplierPaymentRow[]){return [['instruction_id','invoice','date','currency','amount','company_bank_ledger','beneficiary','beneficiary_account'].map(csvCell).join(','),...items.map(i=>[i.id,i.invoice_number,batch.planned_date,batch.currency,minorToDecimal(Number(i.amount_minor)),batch.bank_account_id,i.beneficiary_name,i.beneficiary_account].map(csvCell).join(','))].join('\r\n');}
export function supplierBatchWorkItems(rows:SupplierPaymentRow[],authority:{canWrite:boolean;canApprove:boolean;canPay:boolean;samePerson(id:unknown):boolean}):WorkItem[]{
 return rows.map(b=>{
  const needsRetry=Number(b.failed_count)>0;
  const completed=b.status==='cancelled'||b.status==='exported'&&Number(b.pending_count)===0&&!needsRetry;
  const canAct=!completed&&(needsRetry&&authority.canWrite|| (b.status==='draft'?authority.canApprove&&!authority.samePerson(b.created_by):authority.canPay&&!authority.samePerson(b.created_by)&&!authority.samePerson(b.approved_by)||b.status==='exported'&&authority.canApprove&&!authority.samePerson(b.exported_by)));
  return {id:String(b.id),type:'supplier_batch',actionKind:b.status==='draft'&&!needsRetry&&canAct?'approval':'work',title:'Supplier payment batch',detail:`${b.planned_date} · ${b.currency} · ${b.pending_count} pending · ${b.failed_count} failed with an unpaid source${Number(b.unknown_count)?' · Bank result uncertain':''}`,href:`/finance/supplier-batches/${b.id}`,status:completed?'completed':String(b.status),updatedAt:Number(b.updated_at),queue:completed?'completed':canAct?'todo':'waiting',canAct,responsibleQueue:needsRetry?'Company Finance preparer':b.status==='draft'?'Independent Finance approver':'Company Treasury / Finance review',nextAction:needsRetry?'Review failed instructions and prepare payment for the remaining source balance':b.status==='draft'?'Review invoices, amounts and beneficiary details':'Review export and each bank result',relationships:[canAct?'approval':'team']};
 });
}
