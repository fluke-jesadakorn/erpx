// Test-only retained schema oracle; never imported by product/runtime code.
// Exact original source: packages/server-commerce/src/domain/service-parts-posting-setup.ts
// Original SHA-256: 081a157c081897b7a53d4815e3b70fc040d2e4199022b5ba7b5d9f30c969fefb
import {z} from 'zod';
import {commercePostingSetupSchema,type CommercePostingSetupConfiguration,type CommercePostingSetupChart,type CommercePostingSetupAccount} from '@erpx/server-commerce/domain/posting-setup';
const salesDefaultsIdentity=z.object({id:z.string().min(1).max(180),version:z.number().int().positive()}).strict();
// Absence remains absent in parsed historical requests; normalizing only for
// validation preserves exact original no-default command hashes on replay.
export const servicePartsPostingSetupSchema=commercePostingSetupSchema.extend({expectedSalesDefaults:salesDefaultsIdentity.nullable().optional()}).strict();
export const servicePartsExistingPostingSetupSchema=z.object({expectedVersion:z.literal(0),contextKey:z.string().regex(/^[a-f0-9]{64}$/),reason:z.string().trim().min(3).max(1000)}).strict();
export type ServicePartsPostingSetupInput=z.infer<typeof servicePartsPostingSetupSchema>;
export type ServicePartsExistingPostingSetupInput=z.infer<typeof servicePartsExistingPostingSetupSchema>;
export type ServicePartsPostingSetupResult={id:string;status:'configured'|'confirmed-existing';version:1};
export type ServicePartsApprovedSalesDefaults={id:string;version:number;chartId:string;currency:string;receivableAccountId:string;revenueAccountId:string;taxAccountId:string|null};
export type ServicePartsPostingSetupView={expectedSalesDefaults?:{id:string;version:number}|null;approvedSalesDefaults?:ServicePartsApprovedSalesDefaults|null;companyId:string;available:boolean;schemaAvailable:boolean;configured:boolean|null;sourceReady:boolean;version:0|1|null;canConfigure:boolean;canConfirmExisting:boolean;contextKey:string|null;configuration:CommercePostingSetupConfiguration|null;charts:CommercePostingSetupChart[];accounts:CommercePostingSetupAccount[];unavailableReason:'authority-required'|'purchase-required'|'schema-required'|'history-requires-review'|'invalid-existing-mapping'|'sales-defaults-required'|null};
export type ServicePartsStockWorkspace={companyId:string;available:boolean;schemaAvailable:boolean;sourceReady:boolean;currency:string|null;canCreateWarehouse:boolean;canCreateProduct:boolean;branches:{id:string;name:string}[];unavailableReason:'stock-authority-required'|'purchase-required'|'schema-required'|'service-posting-required'|null};
