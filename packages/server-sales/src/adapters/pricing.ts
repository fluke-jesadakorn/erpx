import {salesPricingPurchaseControls} from '@erpx/server-sales/adapters/pricing-purchases';
import {priceListSchema,priceListUpdateSchema,priceListDecisionSchema} from '@erpx/server-sales/domain/price-lists';
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import {salesPricingCurrency,salesPricingProducts,salesPricingProduct} from "@erpx/server-commerce/public/sales-order-sources";
import {salesPricingCustomers,salesPricingCustomerValid} from "@erpx/server-finance/public/sales-customer-sources";
import {createSalesPricingApplication} from "@erpx/server-sales/application/pricing";
import type {SalesPricingInput} from "@erpx/server-sales/domain/pricing";
import {BusinessRuleError} from "@erpx/server-shared/index";
import 'server-only';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {ApiError} from "@erpx/server-runtime/public/http";
import {replayCommand} from "@erpx/server-workflow/public/commands";


import {createPriceListApplication,type PriceListRepository} from '@erpx/server-sales/application/price-lists';
export {priceListSchema,priceListUpdateSchema,priceListDecisionSchema} from '@erpx/server-sales/domain/price-lists';
type Row=Record<string,string|number|null>;
function access(actor:WorkContext){requirePermission(actor,'commerce.read');requirePermission(actor,'commerce.sales');}
function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);return createPriceListApplication({companyScope:!actor.branchId&&!actor.departmentId,authorize:()=>access(actor),authorizeReview:()=>{requirePermission(actor,'commerce.approve');},canApprove:hasPermission(actor,'commerce.approve'),isSamePerson:id=>samePerson(actor,id),clock:{now:()=>Date.now()},repository:async()=>repository(await database(),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):PriceListRepository{return {
 async workspace(){const [lists,items,products,customers,settings]=await Promise.all([
 write.prepare(`SELECT l.*,p.name customer_name FROM sales_price_lists l LEFT JOIN (${financeProjectionSources.finance_parties}) p ON p.id=l.customer_id WHERE l.company_id=? ORDER BY l.created_at DESC`).bind(actor.companyId).all<Row>(),
 write.prepare(`SELECT i.*,p.sku,p.name product_name FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=i.product_id WHERE l.company_id=? ORDER BY i.price_list_id,i.ordinal`).bind(actor.companyId).all<Row>(),
 salesPricingProducts(write,actor),
 salesPricingCustomers(write,actor),
 salesPricingCurrency(write,actor),
]);return {lists:lists.results,items:items.results,products:products.results,customers:customers.results,settings};},
 replay:(key,operation,id,input)=>replayCommand(write,actor,key,operation,id,input),
 list:id=>write.prepare('SELECT * FROM sales_price_lists WHERE id=? AND company_id=?').bind(id,actor.companyId).first<Row>(),
 currency:()=>salesPricingCurrency(write,actor),
 customer:async(id,currency)=>salesPricingCustomerValid(write,actor,id,currency),
 product:id=>salesPricingProduct(write,actor,id),
 commit:async()=>{throw new ApiError(503,'NATIVE_SALES_PRICING_UNAVAILABLE','The native Sales pricing service is unavailable.');},
};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function pricingWorkspace(actor:WorkContext){return translated(async()=>{const database=(await holdingResources(actor.holdingId)).write,purchase=await salesPricingPurchaseControls(database,actor),view=await application(actor,database).workspace();return {...view,...(purchase?{purchase}: {})};});}
export async function resolveSalesPricing(db:SqlDatabase,actor:WorkContext,input:SalesPricingInput){
 const application=createSalesPricingApplication({
 async list(input){if(input.priceListId)return await db.prepare(`SELECT * FROM sales_price_lists WHERE id=? AND company_id=? AND status='active' AND currency=(SELECT currency FROM (${commerceProjectionSources.commerce_settings}) commerce_settings WHERE company_id=?) AND valid_from<=? AND valid_until>=? AND (customer_id IS NULL OR customer_id=?)`).bind(input.priceListId,actor.companyId,actor.companyId,input.orderDate,input.orderDate,input.partyId).first<Row>()??null;
 return await db.prepare(`SELECT * FROM sales_price_lists WHERE company_id=? AND status='active' AND currency=(SELECT currency FROM (${commerceProjectionSources.commerce_settings}) commerce_settings WHERE company_id=?) AND valid_from<=? AND valid_until>=? AND (customer_id=? OR customer_id IS NULL) ORDER BY CASE WHEN customer_id=? THEN 0 ELSE 1 END,valid_from DESC,activated_at DESC LIMIT 1`).bind(actor.companyId,actor.companyId,input.orderDate,input.orderDate,input.partyId,input.partyId).first<Row>()??null;},
 async items(id){return(await db.prepare('SELECT * FROM sales_price_list_items WHERE price_list_id=?').bind(id).all<Row>()).results;}
 });try{return await application(input);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}

