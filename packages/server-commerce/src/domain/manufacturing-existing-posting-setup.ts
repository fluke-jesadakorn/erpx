import {z} from 'zod';
import type {CommercePostingSetupConfiguration} from '@erpx/server-commerce/domain/posting-setup';

/** The initial CAS belongs to this confirmation, never to the unversioned
 * original settings row. Its complete image is bound by contextKey. */
export const manufacturingExistingPostingSetupSchema=z.object({expectedVersion:z.literal(0),contextKey:z.string().regex(/^[a-f0-9]{64}$/),reason:z.string().trim().min(3).max(1000)}).strict();
export type ManufacturingExistingPostingSetupInput=z.infer<typeof manufacturingExistingPostingSetupSchema>;
export type ManufacturingExistingPostingSetupResult={id:string;status:'confirmed-existing';version:1};
export type ManufacturingExistingPostingSetupView={
 companyId:string;available:boolean;schemaAvailable:boolean;configured:boolean|null;confirmed:boolean|null;version:0|1|null;
 canConfirm:boolean;contextKey:string|null;configuration:CommercePostingSetupConfiguration|null;
 previewChart:{id:string;name:string;baseCurrency:string}|null;previewAccounts:{id:string;code:string;name:string;currency:string}[];
 effectiveSalesConfiguration:{chartId:string;currency:string;receivableAccountId:string;revenueAccountId:string;outputTaxAccountId:string|null}|null;
 expectedSalesDefaults:{id:string;version:number}|null;
 unavailableReason:'authority-required'|'purchase-required'|'schema-required'|'mapping-required'|'invalid-existing-mapping'|'already-manufacturing-configured'|null;
};
