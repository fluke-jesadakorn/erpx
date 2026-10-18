import {z} from 'zod';
import {createChartSchema,createAccountSchema,createPeriodSchema} from './finance';
import {manualRateSchema} from './company-money';
import {salesDefaultsSchema,salesDefaultsDecisionSchema} from './sales';

const contextKey=z.string().regex(/^[a-f0-9]{64}$/);
const initial={expectedVersion:z.literal(0),contextKey};
export const salesPostingChartSchema=z.object({...initial,input:createChartSchema}).strict();
export const salesPostingAccountSchema=z.object({...initial,input:createAccountSchema}).strict();
export const salesPostingPeriodSchema=z.object({...initial,input:createPeriodSchema}).strict();
export const salesPostingRateSchema=z.object({...initial,input:manualRateSchema}).strict();
export const salesPostingDefaultsSchema=z.object({...initial,input:salesDefaultsSchema}).strict();
export const salesPostingDecisionSchema=z.object({contextKey,input:salesDefaultsDecisionSchema}).strict();
export type SalesPostingChartInput=z.infer<typeof salesPostingChartSchema>;
export type SalesPostingAccountInput=z.infer<typeof salesPostingAccountSchema>;
export type SalesPostingPeriodInput=z.infer<typeof salesPostingPeriodSchema>;
export type SalesPostingRateInput=z.infer<typeof salesPostingRateSchema>;
export type SalesPostingDefaultsInput=z.infer<typeof salesPostingDefaultsSchema>;
export type SalesPostingDecisionInput=z.infer<typeof salesPostingDecisionSchema>;
export type SalesPostingSetupView={
 companyId:string;available:boolean;contextKey:string|null;
 unavailableReason:'authority-required'|'purchase-required'|null;
 canCreateChart:boolean;canCreateAccount:boolean;canCreatePeriod:boolean;canConfirmRate:boolean;canPrepareDefaults:boolean;
 charts:{id:string;name:string;baseCurrency:string}[];
 accounts:{id:string;chartId:string;code:string;name:string;category:string;currency:string;controlKind:string}[];
 periods:{id:string;name:string;startDate:string;endDate:string;state:string;version:number}[];
 rates:{id:string;currency:string;effectiveDate:string;currencyPerUsd:string;confirmedAt:number}[];
 defaults:{id:string;chartId:string;currency:string;receivableAccountId:string;revenueAccountId:string;taxAccountId:string|null;status:'draft'|'approved'|'rejected'|'superseded';version:number;reason:string;reviewReason:string|null;canReview:boolean;canApprove:boolean;canReject:boolean;usable:boolean}[];
};
