import {z} from 'zod';
import {createChartSchema,createAccountSchema,createPeriodSchema} from '@erpx/server-finance/domain/finance';
import {manualRateSchema} from '@erpx/server-finance/domain/company-money';
import type {CommercePostingSetupChart,CommercePostingSetupAccount} from '@erpx/server-commerce/public/posting-setup';

const envelope={expectedVersion:z.literal(0),contextKey:z.string().regex(/^[a-f0-9]{64}$/)};
export const supplyPostingChartSchema=z.object({...envelope,input:createChartSchema}).strict();
export const supplyPostingAccountSchema=z.object({...envelope,input:createAccountSchema}).strict();
export const supplyPostingPeriodSchema=z.object({...envelope,input:createPeriodSchema}).strict();
export const supplyPostingRateSchema=z.object({...envelope,input:manualRateSchema}).strict();
export type SupplyPostingChartInput=z.infer<typeof supplyPostingChartSchema>;
export type SupplyPostingAccountInput=z.infer<typeof supplyPostingAccountSchema>;
export type SupplyPostingPeriodInput=z.infer<typeof supplyPostingPeriodSchema>;
export type SupplyPostingRateInput=z.infer<typeof supplyPostingRateSchema>;
export type SupplyPostingPrerequisitesView={
 companyId:string;available:boolean;configured:boolean|null;contextKey:string|null;
 canCreateChart:boolean;canCreateAccount:boolean;canCreatePeriod:boolean;canConfirmRate:boolean;
 charts:CommercePostingSetupChart[];accounts:CommercePostingSetupAccount[];
 periods:{id:string;name:string;startDate:string;endDate:string;state:string;version:number}[];
 rates:{id:string;currency:string;effectiveDate:string;currencyPerUsd:string;confirmedAt:number}[];
 unavailableReason:'authority-required'|'purchase-required'|'history-requires-review'|null;
};
