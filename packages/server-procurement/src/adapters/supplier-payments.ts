import {withSupplierPaymentPurchaseRead,supplierPaymentWorkAccess,supplierPaymentSourceOptions} from "@erpx/server-finance/public/supplier-payment-purchases";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {supplierPaymentRepository} from "@erpx/server-finance/public/supplier-payment-repository";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createSupplierPaymentApplication,type SupplierPaymentReadPorts} from "@erpx/server-procurement/application/supplier-payments";
export {supplierBatchSchema,supplierDecisionSchema,supplierExportSchema,supplierResultSchema,supplierConfirmSchema} from "@erpx/server-procurement/domain/supplier-payments";
function application(actor:WorkContext,provided?:SqlDatabase,sourceInvoiceId?:string){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(actor.holdingId).then(x=>x.write),repository=async()=>supplierPaymentRepository(await database(),actor,{invoiceId:sourceInvoiceId});
 const ports:SupplierPaymentReadPorts={userId:actor.userId,branchId:actor.branchId,departmentId:actor.departmentId,canRead:hasPermission(actor,'finance.read'),canWrite:hasPermission(actor,'finance.write'),canApprove:hasPermission(actor,'finance.approve'),canPay:hasPermission(actor,'finance.pay'),samePerson:(id)=>samePerson(actor,id),
 async authorizeRead(){requirePermission(actor,'finance.read');},
 batch:async(id)=>(await repository()).batch(id),item:async(id)=>(await repository()).item(id),list:async(forWork)=>(await repository()).list(forWork),workspace:async(id,batch)=>(await repository()).workspace(id,batch),
  };return createSupplierPaymentApplication(ports);
}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export const getSupplierBatch=(a:WorkContext,id:string)=>withSupplierPaymentPurchaseRead(a,db=>invoke(()=>application(a,db).get(id)),{type:"batch",id});
export const listSupplierBatches=(a:WorkContext,forWork=false)=>withSupplierPaymentPurchaseRead(a,db=>invoke(()=>application(a,db).list(forWork)));
export const supplierBatchWorkspace=(a:WorkContext,id?:string)=>withSupplierPaymentPurchaseRead(a,async db=>{const database=db??(await holdingResources(a.holdingId)).write,data=await invoke(()=>application(a,db).workspace(id)),options=await supplierPaymentSourceOptions(database,a,id?{type:"batch",id}:undefined);return {...data,sourcePurchase:options.purchase};},id?{type:"batch",id}:undefined);
export const supplierInvoiceBatchWorkspace=(a:WorkContext,invoiceId:string)=>withSupplierPaymentPurchaseRead(a,async db=>{const database=db??(await holdingResources(a.holdingId)).write,data=await invoke(()=>application(a,db,invoiceId).workspace()),options=await supplierPaymentSourceOptions(database,a,{type:"invoice",id:invoiceId});return {...data,sourcePurchase:options.purchase};},{type:"invoice",id:invoiceId});
export const supplierBatchWork=async(a:WorkContext)=>{const access=await supplierPaymentWorkAccess(a);if(!access.history)return [];const items=await invoke(()=>application(a).work());return access.settleExisting?items:items.map(item=>({...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,nextAction:"Review supplier payment history",relationships:["team" as const]}));};
