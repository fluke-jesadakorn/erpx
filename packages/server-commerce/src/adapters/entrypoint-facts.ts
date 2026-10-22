import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Closed Commerce relations used only by native owner projections. */
export const commerceEntrypointReadSources={settings:'SELECT company_id,chart_id FROM commerce_settings',warehouses:'SELECT id,branch_id FROM commerce_warehouses',products:'SELECT id,sku,name FROM commerce_products'} as const;
export function commerceEntrypointFacts(db:SqlDatabase){return {
 orderKind:(id:string,companyId:string)=>db.prepare('SELECT kind FROM commerce_orders WHERE id=? AND company_id=?').bind(id,companyId).first<{kind:string}>(),
 fulfillmentKind:(id:string,companyId:string)=>db.prepare('SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=?').bind(id,companyId).first<{kind:string}>(),
 invoiceKind:(id:string,companyId:string)=>db.prepare('SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.invoice_id=? AND o.company_id=?').bind(id,companyId).first<{kind:string}>(),
 returnKind:(id:string,companyId:string)=>db.prepare('SELECT o.kind FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id WHERE r.id=? AND o.company_id=?').bind(id,companyId).first<{kind:string}>()
};}
