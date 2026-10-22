import {z} from 'zod';

const accountId=z.string().trim().min(1).max(180);
export const commercePostingSetupSchema=z.object({
 expectedVersion:z.literal(0),contextKey:z.string().regex(/^[a-f0-9]{64}$/),
 chartId:accountId,currency:z.string().regex(/^[A-Z]{3}$/),reason:z.string().trim().min(3).max(1000),
 receivableAccountId:accountId,payableAccountId:accountId,revenueAccountId:accountId,
 inventoryAccountId:accountId,clearingAccountId:accountId,cogsAccountId:accountId,
 inputTaxAccountId:accountId,outputTaxAccountId:accountId,cashAccountId:accountId,
}).strict();
export type CommercePostingSetupInput=z.infer<typeof commercePostingSetupSchema>;
export type CommercePostingSetupConfiguration=Omit<CommercePostingSetupInput,'expectedVersion'|'contextKey'|'reason'>;
export type CommercePostingSetupResult={id:string;status:'configured';version:1};
export type CommercePostingSetupChart={id:string;name:string;baseCurrency:string};
export type CommercePostingSetupAccount={id:string;chartId:string;code:string;name:string;category:string;currency:string;controlKind:string};
export type CommercePostingSetupView={
 companyId:string;available:boolean;configured:boolean|null;version:0|1|null;canConfigure:boolean;
 contextKey:string|null;configuration:CommercePostingSetupConfiguration|null;
 charts:CommercePostingSetupChart[];accounts:CommercePostingSetupAccount[];
 unavailableReason:'authority-required'|'purchase-required'|null;
};
