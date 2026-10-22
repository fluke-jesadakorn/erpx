import {z} from 'zod';
/** Only categories actually emitted by the original approved Payroll accrual. */
export const payrollOtherLiabilityKindSchema=z.enum(['benefit','deduction_other','employer_other']);
const selection={kind:payrollOtherLiabilityKindSchema,liabilityAccountId:z.string().min(1).max(180),expectedRunVersion:z.number().int().positive(),contextKey:z.string().regex(/^[a-f0-9]{64}$/),reason:z.string().trim().min(3).max(3000)};
export const payrollOtherRemittanceSchema=z.object({...selection,amountMinor:z.number().int().positive().safe(),bankAccountId:z.string().min(1).max(180),bankTransactionId:z.string().min(1).max(256),bankVersion:z.number().int().positive(),bankReference:z.string().min(1).max(160)}).strict();
export const payrollOtherBankEvidenceSchema=z.object({...selection,bankAccount:z.string().min(1).max(180),statementNumber:z.string().trim().min(1),statementDate:z.string().date(),opening:z.string().min(1),closing:z.string().min(1),closingFxRate:z.string().optional(),sourceHash:z.string().min(8),rows:z.array(z.object({externalId:z.string().trim().min(1).max(160),transactionDate:z.string().date(),description:z.string().optional(),amount:z.string().min(1),currency:z.string().length(3)}).strict()).length(1)}).strict();
export const payrollOtherBankEvidenceIgnoreSchema=z.object({...selection,bankTransactionId:z.string().min(1).max(256),bankVersion:z.number().int().positive()}).strict();
export type PayrollOtherBankEvidenceIgnoreInput=z.infer<typeof payrollOtherBankEvidenceIgnoreSchema>;
export type PayrollOtherBankEvidenceIgnoreResult={id:string;status:'ignored';version:number;payrollRunId:string;kind:z.infer<typeof payrollOtherLiabilityKindSchema>;liabilityAccountId:string};
export type PayrollOtherLiabilityKind=z.infer<typeof payrollOtherLiabilityKindSchema>;
export type PayrollOtherRemittanceInput=z.infer<typeof payrollOtherRemittanceSchema>;
export type PayrollOtherBankEvidenceInput=z.infer<typeof payrollOtherBankEvidenceSchema>;
export const payrollOtherDescriptions:Record<PayrollOtherLiabilityKind,string>={benefit:'Payroll benefit',deduction_other:'Payroll deduction other',employer_other:'Payroll employer other'};
export type PayrollOtherLiability={kind:PayrollOtherLiabilityKind;accountId:string;accountCode:string;accountName:string;accruedMinor:number;remittedMinor:number;outstandingMinor:number};
export type PayrollOtherBankEvidence={id:string;version:number;kind:PayrollOtherLiabilityKind;liabilityAccountId:string;bankAccountId:string;transactionDate:string;reference:string;amountMinor:number;currency:string;matchStatus:string;canRemit:boolean;canIgnore?:boolean};
export type PayrollOtherLiabilitiesView={companyId:string;runId:string;runVersion:number;currency:string;schemaAvailable:boolean;contextKey:string|null;canRemit:boolean;canImportBankEvidence:boolean;obligations:PayrollOtherLiability[];bankAccounts:{id:string;code:string;name:string;currency:string}[];bankEvidence:PayrollOtherBankEvidence[];unavailableReason:'schema-required'|null};
export type PayrollOtherRemittanceResult={id:string;runId:string;kind:PayrollOtherLiabilityKind;liabilityAccountId:string;amountMinor:number;journalId:string;remainingMinor:number};
export type PayrollOtherBankEvidenceResult={id:string;imported:1;status:'imported';payrollRunId:string;kind:PayrollOtherLiabilityKind;liabilityAccountId:string;bankTransactionId:string};
