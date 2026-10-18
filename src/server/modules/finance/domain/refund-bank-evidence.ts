import {z} from 'zod';
import {createFinancePolicy} from './finance-policy';
import type {FinanceError} from './invoice-line-revision';

/** This transport contains bank observations, never an owner or purchase label. */
export const refundBankEvidenceSchema=z.object({
 expectedVersion:z.number().int().positive(),
 bankAccount:z.string().min(1),
 statementNumber:z.string().trim().min(1),
 statementDate:z.string().date(),
 opening:z.string().min(1),closing:z.string().min(1),
 closingFxRate:z.string().optional(),
 sourceHash:z.string().min(8),
 rows:z.array(z.object({
  externalId:z.string().trim().min(1),transactionDate:z.string().date(),
  description:z.string().optional(),amount:z.string().min(1),currency:z.string().length(3),
 }).strict()).length(1),
}).strict();
export type RefundBankEvidenceInput=z.infer<typeof refundBankEvidenceSchema>;
/** Facts come only from the original owned instruction and obligation reader. */
export type RefundBankEvidenceFacts={
 instructionId:string;obligationId:string;version:number;instructionStatus:string;
 obligationStatus:string;currency:string;direction:'outgoing'|'incoming';
 amountMinor:number;cashAccountId:string;bankAccount:string;plannedDate:string;
};
export function validateRefundBankEvidenceInput(error:FinanceError,facts:RefundBankEvidenceFacts,input:RefundBankEvidenceInput){
 if(facts.version!==input.expectedVersion)throw error(409,'STALE_VERSION','The actual refund instruction changed. Refresh its evidence.');
 if(!['exported','unknown'].includes(facts.instructionStatus)||facts.obligationStatus!=='approved')throw error(409,'REFUND_BANK_EVIDENCE_NOT_READY','Use an exported refund instruction whose original obligation remains approved.');
 const row=input.rows[0],amount=createFinancePolicy(error).decimalToMinor(row.amount);
 if(!Number.isSafeInteger(facts.amountMinor)||facts.amountMinor<=0||
  facts.bankAccount!==facts.cashAccountId||input.bankAccount!==facts.cashAccountId||
  row.currency.toUpperCase()!==facts.currency||amount!==(facts.direction==='outgoing'?-facts.amountMinor:facts.amountMinor)||
  row.transactionDate<facts.plannedDate||row.transactionDate>input.statementDate){
  throw error(422,'REFUND_BANK_EVIDENCE_MISMATCH','The bank observation must match this exact instruction account, direction, amount, currency and planned date.');
 }
}
