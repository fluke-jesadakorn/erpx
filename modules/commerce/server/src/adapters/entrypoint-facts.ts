import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeCommerceEntrypointFacts,nativeCommerceEntrypointKind} from './native-lane-13/client';
/** Closed Commerce relations used only by native owner projections. */
export const commerceEntrypointReadSources={settings:'SELECT company_id,chart_id FROM commerce_settings',warehouses:'SELECT id,branch_id FROM commerce_warehouses',products:'SELECT id,sku,name FROM commerce_products'} as const;
/**
 * Native owner entrypoint identity reads. The `database` argument is retained
 * only for call-site compatibility; no statement is prepared or executed on it.
 * The mounted axum owner router owns all five queries, Company scoping and the
 * response projection. A missing or failing native runtime fails closed with
 * ApiError(503, ...); there is no TypeScript fallback.
 */
export function commerceEntrypointFacts(_db:SqlDatabase){return {
 orderKind:(id:string,companyId:string)=>nativeCommerceEntrypointKind('orderKind',id,companyId),
 orderFacts:(id:string,companyId:string)=>nativeCommerceEntrypointFacts(id,companyId),
 fulfillmentKind:(id:string,companyId:string)=>nativeCommerceEntrypointKind('fulfillmentKind',id,companyId),
 invoiceKind:(id:string,companyId:string)=>nativeCommerceEntrypointKind('invoiceKind',id,companyId),
 returnKind:(id:string,companyId:string)=>nativeCommerceEntrypointKind('returnKind',id,companyId)
};}
