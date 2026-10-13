import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "../../finance/public/commercial-owner-projection-sources";
import {salesPricingCurrency,salesPricingProducts,salesPricingProduct} from "../../commerce/public/sales-order-sources";
import {salesPricingCustomers,salesPricingCustomerValid} from "../../finance/public/sales-customer-sources";
import {createSalesPricingApplication} from "@/server/modules/sales/application/pricing";
import type {SalesPricingInput} from "@/server/modules/sales/domain/pricing";
import {BusinessRuleError} from "@/server/modules/shared/index";
import 'server-only';
import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {SqlDatabase} from "@/server/modules/runtime/public/runtime/types";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {hasPermission} from "@/lib/identity_access/permissions";
import {samePerson} from "@/lib/identity_access/iam";
import {ApiError} from "@/server/modules/runtime/public/http";
import {assertion,executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";


import {createPriceListApplication,type PriceListRepository,type PriceListMutation} from '../application/price-lists';
import type {PriceListInput,PriceListUpdate,PriceListDecision} from '../domain/price-lists';
export {priceListSchema,priceListUpdateSchema,priceListDecisionSchema} from '../domain/price-lists';
type Row=Record<string,string|number|null>;
function access(actor:WorkContext){requirePermission(actor,'commerce.read');requirePermission(actor,'commerce.sales');}
function application(actor:WorkContext){let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);return createPriceListApplication({companyScope:!actor.branchId&&!actor.departmentId,authorize:()=>access(actor),authorizeReview:()=>{requirePermission(actor,'commerce.approve');},canApprove:hasPermission(actor,'commerce.approve'),isSamePerson:id=>samePerson(actor,id),clock:{now:()=>Date.now()},repository:async()=>repository(await database(),actor)});}
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
 commit:mutation=>commit(write,actor,mutation),
};}
async function commit(write:SqlDatabase,actor:WorkContext,mutation:PriceListMutation){const {key,id,now}=mutation;switch(mutation.action){
 case 'create':{const {input,products,prices}=mutation;const steps=[write.prepare("INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,max_auto_discount_bps,created_by,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,input.name,input.currency,input.customerId,input.validFrom,input.validUntil,input.maxAutoDiscountBps,actor.userId,actor.userId,now,now)];input.items.forEach((item,index)=>steps.push(assertion(write,key,`product-${index}`,`EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_products}) commerce_products WHERE id=? AND company_id=? AND version=? AND status='active')`,[item.productId,actor.companyId,products[index].version]),write.prepare('INSERT INTO sales_price_list_items(id,price_list_id,product_id,price_minor,tax_bps,ordinal) VALUES(?,?,?,?,?,?)').bind(`${id}-item-${index}`,id,item.productId,prices[index],item.taxBps,index)));
 return executeCommand(write,actor,key,'sales.price-list.create',input,id,1,{id,status:'draft',version:1},steps,{companyId:actor.companyId,branchId:null,departmentId:null});}
 case 'update':{const {input,products,prices}=mutation,operation=mutation.operation;const nextVersion=input.expectedVersion+1,steps=[assertion(write,key,'version',"EXISTS(SELECT 1 FROM sales_price_lists WHERE id=? AND company_id=? AND version=? AND status='draft')",[id,actor.companyId,input.expectedVersion]),write.prepare('UPDATE sales_price_lists SET name=?,currency=?,customer_id=?,valid_from=?,valid_until=?,max_auto_discount_bps=?,prepared_by=?,version=?,updated_at=? WHERE id=?').bind(input.name,input.currency,input.customerId,input.validFrom,input.validUntil,input.maxAutoDiscountBps,actor.userId,nextVersion,now,id),write.prepare('DELETE FROM sales_price_list_items WHERE price_list_id=?').bind(id)];input.items.forEach((item,index)=>steps.push(assertion(write,key,`product-${index}`,`EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_products}) commerce_products WHERE id=? AND company_id=? AND version=? AND status='active')`,[item.productId,actor.companyId,products[index].version]),write.prepare('INSERT INTO sales_price_list_items(id,price_list_id,product_id,price_minor,tax_bps,ordinal) VALUES(?,?,?,?,?,?)').bind(`${id}-v${nextVersion}-item-${index}`,id,item.productId,prices[index],item.taxBps,index)));return executeCommand(write,actor,key,operation,input,id,nextVersion,{id,status:'draft',version:nextVersion},steps,{companyId:actor.companyId,branchId:null,departmentId:null});}
 case 'decide':{const input=mutation.input,operation=mutation.operation;const status=input.action==='activate'?'active':'cancelled',steps=[assertion(write,key,'version',"EXISTS(SELECT 1 FROM sales_price_lists WHERE id=? AND company_id=? AND version=? AND status='draft')",[id,actor.companyId,input.expectedVersion])];if(input.action==='activate')steps.push(write.prepare("UPDATE sales_price_lists SET status='active',approved_by=?,approval_reason=?,activated_at=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,input.reason,now,now,id));else steps.push(write.prepare("UPDATE sales_price_lists SET status='cancelled',approval_reason=?,version=version+1,updated_at=? WHERE id=?").bind(input.reason,now,id));return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1},steps,{companyId:actor.companyId,branchId:null,departmentId:null});}
}}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function pricingWorkspace(actor:WorkContext){return translated(()=>application(actor).workspace());}
export function savePriceList(actor:WorkContext,input:PriceListInput,key:string){return translated(()=>application(actor).create(input,key));}
export function updatePriceList(actor:WorkContext,id:string,input:PriceListUpdate,key:string){return translated(()=>application(actor).update(id,input,key));}
export function decidePriceList(actor:WorkContext,id:string,input:PriceListDecision,key:string){return translated(()=>application(actor).decide(id,input,key));}
export async function resolveSalesPricing(db:SqlDatabase,actor:WorkContext,input:SalesPricingInput){
 const application=createSalesPricingApplication({
 async list(input){if(input.priceListId)return await db.prepare(`SELECT * FROM sales_price_lists WHERE id=? AND company_id=? AND status='active' AND currency=(SELECT currency FROM (${commerceProjectionSources.commerce_settings}) commerce_settings WHERE company_id=?) AND valid_from<=? AND valid_until>=? AND (customer_id IS NULL OR customer_id=?)`).bind(input.priceListId,actor.companyId,actor.companyId,input.orderDate,input.orderDate,input.partyId).first<Row>()??null;
 return await db.prepare(`SELECT * FROM sales_price_lists WHERE company_id=? AND status='active' AND currency=(SELECT currency FROM (${commerceProjectionSources.commerce_settings}) commerce_settings WHERE company_id=?) AND valid_from<=? AND valid_until>=? AND (customer_id=? OR customer_id IS NULL) ORDER BY CASE WHEN customer_id=? THEN 0 ELSE 1 END,valid_from DESC,activated_at DESC LIMIT 1`).bind(actor.companyId,actor.companyId,input.orderDate,input.orderDate,input.partyId,input.partyId).first<Row>()??null;},
 async items(id){return(await db.prepare('SELECT * FROM sales_price_list_items WHERE price_list_id=?').bind(id).all<Row>()).results;}
 });try{return await application(input);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}

