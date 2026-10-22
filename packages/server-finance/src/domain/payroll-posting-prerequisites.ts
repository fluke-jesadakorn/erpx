import {z} from 'zod';
import {createChartSchema,createAccountSchema,createPeriodSchema} from '@erpx/server-finance/domain/finance';
import {manualRateSchema} from '@erpx/server-finance/domain/company-money';

// A selected native pay group has no general version column. The opaque
// context binds its full current owner image; expectedVersion creates a reference.
const id=z.string().trim().min(1).max(180);
export const payrollPostingPrerequisitesSelectionSchema=z.object({payGroupId:id.optional()}).strict();
const envelope={payGroupId:id,expectedVersion:z.literal(0),contextKey:z.string().regex(/^[a-f0-9]{64}$/)};
const account=createAccountSchema.extend({
 category:z.enum(['asset','liability','expense']),
 controlKind:z.enum(['none','bank']).default('none'),
}).strict().superRefine((value,context)=>{
 if(value.controlKind==='bank'&&value.category!=='asset')context.addIssue({code:'custom',path:['controlKind'],message:'A Payroll bank account must be an asset.'});
 if(value.controlKind==='none'&&value.isControl)context.addIssue({code:'custom',path:['isControl'],message:'Use an ordinary non-control Payroll expense, liability or advance account.'});
});
export const payrollPostingChartSchema=z.object({...envelope,input:createChartSchema.strict()}).strict();
export const payrollPostingAccountSchema=z.object({...envelope,input:account}).strict();
export const payrollPostingPeriodSchema=z.object({...envelope,input:createPeriodSchema.strict()}).strict();
export const payrollPostingRateSchema=z.object({...envelope,input:manualRateSchema.strict()}).strict();
export type PayrollPostingPrerequisitesSelection=z.infer<typeof payrollPostingPrerequisitesSelectionSchema>;
export type PayrollPostingChartInput=z.infer<typeof payrollPostingChartSchema>;
export type PayrollPostingAccountInput=z.infer<typeof payrollPostingAccountSchema>;
export type PayrollPostingPeriodInput=z.infer<typeof payrollPostingPeriodSchema>;
export type PayrollPostingRateInput=z.infer<typeof payrollPostingRateSchema>;
export type PayrollPostingChartResult=z.infer<typeof createChartSchema>&{id:string;companyId:string;version:1};
export type PayrollPostingAccountResult=Omit<z.infer<typeof createAccountSchema>,'currency'>&{id:string;companyId:string;currency:string;version:1};
export type PayrollPostingPeriodResult=z.infer<typeof createPeriodSchema>&{id:string;companyId:string;state:'open';version:0};
export type PayrollPostingRateResult={id:string;currency:string;effectiveDate:string;currencyPerUsd:string;confirmedAt:number};
export type PayrollPostingPayGroup={id:string;name:string;currency:string;status:'draft'|'active'|'archived'};
export type PayrollPostingPrerequisitesView={
 companyId:string;available:boolean;contextKey:string|null;
 selection:PayrollPostingPayGroup|null;groups:PayrollPostingPayGroup[];
 canCreateChart:boolean;canCreateAccount:boolean;canCreatePeriod:boolean;canConfirmRate:boolean;
 charts:{id:string;name:string;baseCurrency:string}[];
 accounts:{id:string;chartId:string;code:string;name:string;category:string;currency:string;controlKind:string}[];
 periods:{id:string;name:string;startDate:string;endDate:string;state:string;version:number}[];
 rates:{id:string;currency:string;effectiveDate:string;currencyPerUsd:string;confirmedAt:number}[];
 unavailableReason:'authority-required'|'purchase-required'|'pay-group-required'|'pay-group-unavailable'|'currency-unsupported'|null;
};

/** Receipt-aware confirmation is separate from honest active choice arrays. */
export const payrollPostingPrerequisiteConfirmationSchema=z.discriminatedUnion('kind',[
 z.object({kind:z.literal('chart'),request:payrollPostingChartSchema}).strict(),
 z.object({kind:z.literal('account'),request:payrollPostingAccountSchema}).strict(),
 z.object({kind:z.literal('period'),request:payrollPostingPeriodSchema}).strict(),
 z.object({kind:z.literal('rate'),request:payrollPostingRateSchema}).strict(),
]);
export const payrollPostingPrerequisiteNativeStatusSchema=z.enum(['draft','active','archived','inactive','open','closing','closed','confirmed']);
export type PayrollPostingPrerequisiteConfirmationInput=z.infer<typeof payrollPostingPrerequisiteConfirmationSchema>;
export type PayrollPostingPrerequisiteConfirmationResult={kind:PayrollPostingPrerequisiteConfirmationInput['kind'];commandId:string;payGroupId:string;id:string;status:z.infer<typeof payrollPostingPrerequisiteNativeStatusSchema>;version:number|null};
