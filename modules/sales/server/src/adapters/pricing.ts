import {salesPricingPurchaseControls} from '@erpx/sales-server/adapters/pricing-purchases';
import {priceListSchema,priceListUpdateSchema,priceListDecisionSchema} from '@erpx/sales-server/domain/price-lists';
import {salesPricingCurrency,salesPricingProducts,salesPricingProduct} from "@erpx/commerce-server/public/sales-order-sources";
import {salesPricingCustomers,salesPricingCustomerValid} from "@erpx/finance-server/public/sales-customer-sources";
import {createSalesPricingApplication} from "@erpx/sales-server/application/pricing";
import type {SalesPricingInput} from "@erpx/sales-server/domain/pricing";
import {BusinessRuleError} from "@erpx/platform-shared-server/index";
import 'server-only';
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {samePerson} from "@erpx/identity-access-ui/lib/iam";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {replayCommand} from "@erpx/workflow-server/public/commands";


import {createPriceListApplication,type PriceListRepository} from '@erpx/sales-server/application/price-lists';
import {readPriceListWorkspace} from './native-lane-30/pricing-client';
import {nativeSalesReadAll,nativeSalesReadFirst,SALES_RD_PRICING_LIST_BY_ID,SALES_RD_PRICING_LIST_ACTIVE_BY_ID,SALES_RD_PRICING_LIST_ACTIVE,SALES_RD_PRICING_ITEMS_BY_LIST} from './native-sales-read';
export {priceListSchema,priceListUpdateSchema,priceListDecisionSchema} from '@erpx/sales-server/domain/price-lists';
type Row=Record<string,string|number|null>;
function access(actor:WorkContext){requirePermission(actor,'commerce.read');requirePermission(actor,'commerce.sales');}
function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);return createPriceListApplication({companyScope:!actor.branchId&&!actor.departmentId,authorize:()=>access(actor),authorizeReview:()=>{requirePermission(actor,'commerce.approve');},canApprove:hasPermission(actor,'commerce.approve'),isSamePerson:id=>samePerson(actor,id),clock:{now:()=>Date.now()},repository:async()=>repository(await database(),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):PriceListRepository{return {
 async workspace(){const [products,customers,settings]=await Promise.all([
 salesPricingProducts(write,actor),
 salesPricingCustomers(write,actor),
 salesPricingCurrency(write,actor),
 ]);const native=await readPriceListWorkspace(actor);return {lists:native.lists as Row[],items:native.items as Row[],products:products.results,customers:customers.results,settings};},
 replay:(key,operation,id,input)=>replayCommand(write,actor,key,operation,id,input),
 list:id=>nativeSalesReadFirst<Row>(SALES_RD_PRICING_LIST_BY_ID,[id,actor.companyId],write),
 currency:()=>salesPricingCurrency(write,actor),
 customer:async(id,currency)=>salesPricingCustomerValid(write,actor,id,currency),
 product:id=>salesPricingProduct(write,actor,id),
 commit:async()=>{throw new ApiError(503,'NATIVE_SALES_PRICING_UNAVAILABLE','The native Sales pricing service is unavailable.');},
};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function pricingWorkspace(actor:WorkContext){return translated(async()=>{const database=(await holdingResources(actor.holdingId)).write,purchase=await salesPricingPurchaseControls(database,actor),view=await application(actor,database).workspace();return {...view,...(purchase?{purchase}: {})};});}
export async function resolveSalesPricing(db:SqlDatabase,actor:WorkContext,input:SalesPricingInput){
 const application=createSalesPricingApplication({
 async list(input){if(input.priceListId)return await nativeSalesReadFirst<Row>(SALES_RD_PRICING_LIST_ACTIVE_BY_ID,[input.priceListId,actor.companyId,actor.companyId,input.orderDate,input.orderDate,input.partyId],db)??null;
 return await nativeSalesReadFirst<Row>(SALES_RD_PRICING_LIST_ACTIVE,[actor.companyId,actor.companyId,input.orderDate,input.orderDate,input.partyId,input.partyId],db)??null;},
 async items(id){return(await nativeSalesReadAll<Row>(SALES_RD_PRICING_ITEMS_BY_LIST,[id],db)).results;}
 });try{return await application(input);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}

