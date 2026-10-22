import {z} from 'zod';
import {commercePostingSetupSchema,type CommercePostingSetupConfiguration,type CommercePostingSetupChart,type CommercePostingSetupAccount} from '@erpx/server-commerce/domain/posting-setup';
const salesDefaultsIdentity=z.object({id:z.string().min(1).max(180),version:z.number().int().positive()}).strict();
export const expensePostingSetupSchema=commercePostingSetupSchema.extend({expectedSalesDefaults:salesDefaultsIdentity.nullable().optional()}).strict();
export type ExpensePostingSetupInput=z.infer<typeof expensePostingSetupSchema>;
export type ExpensePostingSetupResult={id:string;status:'configured';version:1};
export type ExpensePostingApprovedSalesDefaults={id:string;version:number;chartId:string;currency:string;receivableAccountId:string;revenueAccountId:string;taxAccountId:string|null};
export type ExpensePostingSetupView={companyId:string;available:boolean;configured:boolean|null;version:0|1|null;canConfigure:boolean;contextKey:string|null;configuration:CommercePostingSetupConfiguration|null;charts:CommercePostingSetupChart[];accounts:CommercePostingSetupAccount[];expectedSalesDefaults:{id:string;version:number}|null;approvedSalesDefaults:ExpensePostingApprovedSalesDefaults|null;unavailableReason:'authority-required'|'purchase-required'|'history-requires-review'|'sales-defaults-required'|'invalid-existing-mapping'|null};
