import {z} from 'zod';
import {commercePostingSetupSchema,type CommercePostingSetupConfiguration,type CommercePostingSetupChart,type CommercePostingSetupAccount,type CommercePostingSetupResult} from '@erpx/server-commerce/domain/posting-setup';
export const salesGoodsPostingSetupSchema=commercePostingSetupSchema.extend({expectedSalesDefaults:z.object({id:z.string().min(1).max(180),version:z.number().int().positive()}).strict().nullable()}).strict();
export type SalesGoodsPostingSetupInput=z.infer<typeof salesGoodsPostingSetupSchema>;
export type SalesGoodsPostingSetupResult=CommercePostingSetupResult;
export type SalesGoodsPostingSetupView={
 companyId:string;available:boolean;configured:boolean|null;version:0|1|null;canConfigure:boolean;contextKey:string|null;
 configuration:CommercePostingSetupConfiguration|null;charts:CommercePostingSetupChart[];accounts:CommercePostingSetupAccount[];
 expectedSalesDefaults:{id:string;version:number}|null;requiresApprovedDefaults:boolean;
 unavailableReason:'authority-required'|'purchase-required'|'goods-history-requires-review'|'sales-defaults-required'|null;
};
