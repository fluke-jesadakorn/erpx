import {supplierBatchSchema,supplierConfirmSchema} from "@erpx/server-procurement/domain/supplier-payments";
import {withSupplierConfirmationMoney} from "@erpx/server-finance/public/supplier-payment-money";
import {withSupplierPaymentPurchaseCommand,withSupplierPaymentPurchaseRead,supplierPaymentWorkAccess,supplierPaymentSourceOptions} from "@erpx/server-finance/public/supplier-payment-purchases";
import "server-only";
import {createHash} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {supplierPaymentRepository,type SupplierPaymentEvidence,type SupplierPaymentJournal} from "@erpx/server-finance/public/supplier-payment-repository";
import {replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createSupplierPaymentApplication,type SupplierPaymentPorts} from "@erpx/server-procurement/application/supplier-payments";
export {supplierBatchSchema,supplierDecisionSchema,supplierExportSchema,supplierResultSchema,supplierConfirmSchema} from "@erpx/server-procurement/domain/supplier-payments";
function application(actor:WorkContext,provided?:SqlDatabase,sourceInvoiceId?:string){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(actor.holdingId).then(x=>x.write),repository=async()=>supplierPaymentRepository(await database(),actor,{invoiceId:sourceInvoiceId});
 const ports:SupplierPaymentPorts<SupplierPaymentEvidence,SupplierPaymentJournal>={clock:{now:()=>Date.now()},companyId:actor.companyId,userId:actor.userId,branchId:actor.branchId,departmentId:actor.departmentId,canRead:hasPermission(actor,'finance.read'),canWrite:hasPermission(actor,'finance.write'),canApprove:hasPermission(actor,'finance.approve'),canPay:hasPermission(actor,'finance.pay'),samePerson:(id)=>samePerson(actor,id),
 async authorizeRead(){requirePermission(actor,'finance.read');},async authorizeWrite(){requirePermission(actor,'finance.write');},async authorizeApprove(){requirePermission(actor,'finance.approve');},async authorizePay(){requirePermission(actor,'finance.pay');},
 replay:async(key,operation,id,input)=>replayCommand(await database(),actor,key,operation,id,input),
 batch:async(id)=>(await repository()).batch(id),item:async(id)=>(await repository()).item(id),list:async(forWork)=>(await repository()).list(forWork),workspace:async(id,batch)=>(await repository()).workspace(id,batch),controlledTaxOwner:async(date)=>(await repository()).controlledTaxOwner(date),bankAccount:async(id)=>(await repository()).bankAccount(id),exportItems:async(id)=>(await repository()).exportItems(id),hash:(content)=>createHash('sha256').update(content,'utf8').digest('hex'),exportReceipt:async(key)=>(await repository()).exportReceipt(key),invoice:async(id)=>(await repository()).invoice(id),bankTransaction:async(id)=>(await repository()).bankTransaction(id),evidenceSets:async(ids)=>(await repository()).evidenceSets(ids),prepareEvidence:async(id,batch,input,key)=>(await repository()).prepareEvidence(id,batch,input,key),prepareJournal:async(invoice,batch,bank,amount,input,id,key)=>(await repository()).prepareJournal(invoice,batch,bank,amount,input,id,key),commit:async(m,result)=>(await repository()).commit(m,result),
 };return createSupplierPaymentApplication(ports);
}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export const getSupplierBatch=(a:WorkContext,id:string)=>withSupplierPaymentPurchaseRead(a,db=>invoke(()=>application(a,db).get(id)),{type:"batch",id});
export const listSupplierBatches=(a:WorkContext,forWork=false)=>withSupplierPaymentPurchaseRead(a,db=>invoke(()=>application(a,db).list(forWork)));
export const supplierBatchWorkspace=(a:WorkContext,id?:string)=>withSupplierPaymentPurchaseRead(a,async db=>{const database=db??(await holdingResources(a.holdingId)).write,data=await invoke(()=>application(a,db).workspace(id)),options=await supplierPaymentSourceOptions(database,a,id?{type:"batch",id}:undefined);return {...data,sourcePurchase:options.purchase};},id?{type:"batch",id}:undefined);
export const supplierInvoiceBatchWorkspace=(a:WorkContext,invoiceId:string)=>withSupplierPaymentPurchaseRead(a,async db=>{const database=db??(await holdingResources(a.holdingId)).write,data=await invoke(()=>application(a,db,invoiceId).workspace()),options=await supplierPaymentSourceOptions(database,a,{type:"invoice",id:invoiceId});return {...data,sourcePurchase:options.purchase};},{type:"invoice",id:invoiceId});
export const createSupplierBatch=(a:WorkContext,raw:unknown,key:string)=>{const input=supplierBatchSchema.parse(raw);return withSupplierPaymentPurchaseCommand(a,key,"finance.supplier_batch.create",tx=>invoke(()=>application(a,tx).create(raw,key)),{type:"new",id:`supplier-batch-${key}`,invoiceIds:input.items.map(item=>item.invoiceId)});};
export const decideSupplierBatch=(a:WorkContext,id:string,raw:unknown,key:string,decision:'approve'|'cancel')=>withSupplierPaymentPurchaseCommand(a,key,`finance.supplier_batch.${decision}`,tx=>invoke(()=>application(a,tx).decide(id,raw,key,decision)),{type:"batch",id});
export const exportSupplierBatch=(a:WorkContext,id:string,raw:unknown,key:string)=>withSupplierPaymentPurchaseCommand(a,key,"finance.supplier_batch.export",tx=>invoke(()=>application(a,tx).export(id,raw,key)),{type:"batch",id});
export const recoverSupplierExport=(a:WorkContext,id:string)=>withSupplierPaymentPurchaseRead(a,db=>invoke(()=>application(a,db).recoverExport(id)),{type:"batch",id});
export const recordSupplierResult=(a:WorkContext,id:string,raw:unknown,key:string)=>withSupplierPaymentPurchaseCommand(a,key,"finance.supplier_batch.result",tx=>invoke(()=>application(a,tx).result(id,raw,key)),{type:"instruction",id});
export const confirmSupplierInstruction=(a:WorkContext,id:string,raw:unknown,key:string)=>{const input=supplierConfirmSchema.parse(raw);return withSupplierConfirmationMoney(a,id,input,key,database=>withSupplierPaymentPurchaseCommand(a,key,"finance.supplier_batch.confirm",tx=>invoke(()=>application(a,tx).confirm(id,input,key)),{type:"instruction",id},database));};
export const supplierBatchWork=async(a:WorkContext)=>{const access=await supplierPaymentWorkAccess(a);if(!access.history)return [];const items=await invoke(()=>application(a).work());return access.settleExisting?items:items.map(item=>({...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,nextAction:"Review supplier payment history",relationships:["team" as const]}));};
