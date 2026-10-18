import {z} from 'zod';
import {productSchema} from './commerce';
/** Original create-only product fields. This source cannot create stock or amend a master. */
export const salesServiceProductSchema=productSchema.safeExtend({itemType:z.literal('service'),trackingMode:z.literal('none').default('none'),expectedVersion:z.never().optional()}).strict();
export type SalesServiceProductInput=z.infer<typeof salesServiceProductSchema>;
export type SalesServiceProductResult={id:string;version:1};
export type SalesServiceProductsView={companyId:string;available:boolean;contextKey:string|null;currency:string|null;canCreate:boolean;products:{id:string;sku:string;name:string;unit:string;priceMinor:number;taxBps:number;status:'active'|'archived';version:number}[];unavailableReason:'authority-required'|'purchase-required'|'sales-posting-required'|null};
