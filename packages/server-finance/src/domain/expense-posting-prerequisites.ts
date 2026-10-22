import {z} from 'zod';
import {createChartSchema,createAccountSchema,createPeriodSchema} from '@erpx/server-finance/domain/finance';
import {manualRateSchema} from '@erpx/server-finance/domain/company-money';

const envelope={expectedVersion:z.literal(0),contextKey:z.string().regex(/^[a-f0-9]{64}$/)};
const account=createAccountSchema.extend({category:z.enum(['asset','liability','revenue','expense'])}).strict().superRefine((value,context)=>{
 const permitted=value.controlKind==='none'||value.controlKind==='payable'&&value.category==='liability'||['receivable','bank'].includes(value.controlKind)&&value.category==='asset';
 if(!permitted)context.addIssue({code:'custom',path:['controlKind'],message:'Retain the original Expense posting role category and control meaning.'});
 if(value.controlKind==='none'&&value.isControl)context.addIssue({code:'custom',path:['isControl'],message:'Use an ordinary non-control account for this posting role.'});
});
export const expensePostingChartSchema=z.object({...envelope,input:createChartSchema.strict()}).strict();
export const expensePostingAccountSchema=z.object({...envelope,input:account}).strict();
export const expensePostingPeriodSchema=z.object({...envelope,input:createPeriodSchema.strict()}).strict();
export const expensePostingRateSchema=z.object({...envelope,input:manualRateSchema.strict()}).strict();
export type ExpensePostingChartInput=z.infer<typeof expensePostingChartSchema>;
export type ExpensePostingAccountInput=z.infer<typeof expensePostingAccountSchema>;
export type ExpensePostingPeriodInput=z.infer<typeof expensePostingPeriodSchema>;
export type ExpensePostingRateInput=z.infer<typeof expensePostingRateSchema>;
export type ExpensePostingChartResult=z.infer<typeof createChartSchema>&{id:string;companyId:string;version:1};
export type ExpensePostingAccountResult=Omit<z.infer<typeof createAccountSchema>,'currency'>&{id:string;companyId:string;currency:string;version:1};
export type ExpensePostingPeriodResult=z.infer<typeof createPeriodSchema>&{id:string;companyId:string;state:'open';version:0};
export type ExpensePostingRateResult={id:string;currency:string;effectiveDate:string;currencyPerUsd:string;confirmedAt:number};
export type ExpensePostingPrerequisitesView={
 companyId:string;available:boolean;configured:boolean|null;contextKey:string|null;
 defaultCurrency:string|null;currencies:string[];
 canCreateChart:boolean;canCreateAccount:boolean;canCreatePeriod:boolean;canConfirmRate:boolean;
 charts:{id:string;name:string;baseCurrency:string}[];
 accounts:{id:string;chartId:string;code:string;name:string;category:string;currency:string;controlKind:string}[];
 periods:{id:string;name:string;startDate:string;endDate:string;state:string;version:number}[];
 rates:{id:string;currency:string;effectiveDate:string;currencyPerUsd:string;confirmedAt:number}[];
 unavailableReason:'authority-required'|'purchase-required'|'history-requires-review'|'sales-defaults-required'|'invalid-existing-mapping'|null;
};
/** Confirmation never inserts an inactive or closed reference into active choices. */
export const expensePostingPrerequisiteConfirmationSchema=z.discriminatedUnion('kind',[
 z.object({kind:z.literal('chart'),request:expensePostingChartSchema}).strict(),
 z.object({kind:z.literal('account'),request:expensePostingAccountSchema}).strict(),
 z.object({kind:z.literal('period'),request:expensePostingPeriodSchema}).strict(),
 z.object({kind:z.literal('rate'),request:expensePostingRateSchema}).strict(),
]);
export const expensePostingPrerequisiteNativeStatusSchema=z.enum(['draft','active','archived','inactive','open','closing','closed','confirmed']);
export type ExpensePostingPrerequisiteConfirmationInput=z.infer<typeof expensePostingPrerequisiteConfirmationSchema>;
export type ExpensePostingPrerequisiteConfirmationResult={kind:ExpensePostingPrerequisiteConfirmationInput['kind'];commandId:string;companyId:string;id:string;status:z.infer<typeof expensePostingPrerequisiteNativeStatusSchema>;version:number|null};
