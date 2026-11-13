import {salesServiceProducts,createSalesServiceProduct,salesServiceProductSchema} from '@erpx/server-commerce/public/sales-service-products';
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {commerceEntrypointFacts} from "@erpx/server-commerce/public/entrypoint-facts";
import {commerceEntrypointFinanceFacts} from "@erpx/server-finance/public/commerce-entrypoint-facts";
import {changeOrderReservation} from '@erpx/server-commerce/public/commerce';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { z } from "zod";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import {commerceOverview,saveOrder,transitionOrder,invoiceFulfillment,settleInvoice,saveMaster,requestCommerceReturn,decideCommerceReturn,completeCommerceReturn,orderSchema,transitionSchema,fulfillmentSchema,invoiceFromDeliverySchema,settlementSchema,returnRequestSchema,returnDecisionSchema,returnCompleteSchema} from '@erpx/server-commerce/public/commerce';
import {nativeCommerceOrderTransition,nativeCommerceOrderSave,nativeCommerceOrderFulfill,nativeCommerceGoodsReceipt,nativeCommerceSalesDelivery} from '@erpx/feature-sales/public/native-commerce-orders-page';
import {nativeSalesConfirmedCancellation} from '@erpx/feature-sales/public/native-sales-documents-page';
import {nativeSalesPricingCommand} from '@erpx/feature-sales/public/native-sales-pricing';
import {getOrder} from '@erpx/server-commerce/public/commerce-order-read';
import {pricingWorkspace} from "@erpx/server-sales/public/sales-pricing";
import {nativeQueueDeliveryNotification,nativeRetryCustomerNotification,nativeUpdateCustomerNotificationSettings} from "@erpx/feature-sales/public/native-sales-notification-commands";
import {nativeSalesNotificationReconcile,salesNotificationReconciliationInput} from "@erpx/feature-sales/public/native-sales-notification-reconciliation";
import { inventoryReorderPolicyActionSchema, inventoryReorderPolicySchema, listInventoryReorderPolicies, saveInventoryReorderPolicy, transitionInventoryReorderPolicy } from "@erpx/server-inventory/public/inventory-reorder-policies";
import {commercePostingSetup,initializeCommercePostingSetup,commercePostingSetupSchema} from "@erpx/server-commerce/public/posting-setup";
import {supplyPostingPrerequisites,createSupplyPostingChart,createSupplyPostingAccount,createSupplyPostingPeriod,confirmSupplyPostingRate,supplyPostingChartSchema,supplyPostingAccountSchema,supplyPostingPeriodSchema,supplyPostingRateSchema} from "@erpx/server-finance/public/supply-posting-prerequisites";
import {salesPostingSetup,createSalesPostingChart,createSalesPostingAccount,createSalesPostingPeriod,confirmSalesPostingRate,prepareSalesPostingDefaults,decideSalesPostingDefaults,salesPostingChartSchema,salesPostingAccountSchema,salesPostingPeriodSchema,salesPostingRateSchema,salesPostingDefaultsSchema,salesPostingDecisionSchema} from "@erpx/server-finance/public/sales-posting-setup";
import {salesGoodsPostingSetup,initializeSalesGoodsPostingSetup,salesGoodsPostingSetupSchema} from "@erpx/server-commerce/public/sales-goods-posting-setup";
type Context={params:Promise<{holdingSlug:string;path:string[]}>};
async function nativePricing(...args:Parameters<typeof nativeSalesPricingCommand>){const data=await nativeSalesPricingCommand(...args);if(data===undefined)throw new ApiError(503,'NATIVE_SALES_PRICING_UNAVAILABLE','The native Sales pricing service is unavailable.');return data;}
async function assertPilotCommercePath(actor:Awaited<ReturnType<typeof requireWorkContext>>,path:string[],input:unknown){
  if(path[0]==='sales-service-products'&&path.length!==1)throw new ApiError(404,'NOT_FOUND','Workflow action not found.');
  if(path[0]==='posting-setup'&&!(path.length===1||path.length===2&&['charts','accounts','periods','rates'].includes(path[1])))throw new ApiError(404,'NOT_FOUND','Workflow action not found.');
  if(path[0]==='sales-posting-setup'&&!(path.length===2&&['charts','accounts','periods','rates','defaults'].includes(path[1])||path.length===4&&path[1]==='defaults'&&path[3]==='decide'))throw new ApiError(404,'NOT_FOUND','Workflow action not found.');
  if(path[0]==='sales-goods-posting-setup'&&path.length!==1)throw new ApiError(404,'NOT_FOUND','Workflow action not found.');
  if(process.env.NODE_ENV!=='production')return;
  const db=(await holdingResources(actor.holdingId)).write;
  const root=path[0];
  if(root==='posting-setup')return assertProductionModuleEnabled(db,actor.companyId,'procurement');
  if(root==='sales-service-products'||root==='sales-posting-setup'||root==='sales-goods-posting-setup')return assertProductionModuleEnabled(db,actor.companyId,'sales');
  if(root==='reorder-policies')return assertProductionModuleEnabled(db,actor.companyId,'inventory');
  if(root==='products'||root==='warehouses')return assertProductionModuleEnabled(db,actor.companyId,'inventory');
  if(root==='pricing'||root==='customer-notifications')return assertProductionModuleEnabled(db,actor.companyId,'sales');
  if(root==='parties'){
    const existing=path[1]?await commerceEntrypointFinanceFacts(db).partyKind(path[1],actor.companyId):null;
    const kind=existing?.kind??(input as {kind?:string})?.kind;
    return assertProductionModuleEnabled(db,actor.companyId,kind==='supplier'?'procurement':'sales');
  }
  let kind:string|undefined;
  if(root==='orders'){
    kind=path[1]&&path.length>2?(await commerceEntrypointFacts(db).orderKind(path[1],actor.companyId))?.kind:(input as {kind?:string})?.kind;
  }else if(root==='fulfillments'){
    kind=(await commerceEntrypointFacts(db).fulfillmentKind(path[1],actor.companyId))?.kind;
  }else if(root==='invoices'){
    kind=(await commerceEntrypointFacts(db).invoiceKind(path[1],actor.companyId))?.kind;
  }else if(root==='returns'){
    kind=(await commerceEntrypointFacts(db).returnKind(path[1],actor.companyId))?.kind;
  }
  return assertProductionModuleEnabled(db,actor.companyId,kind==='purchase'?'procurement':'sales');
}
export async function GET(_request:Request,context:Context) {
  try{const {holdingSlug,path}=await context.params;if(path[0]==='sales-service-products'){if(path.length!==1||new URL(_request.url).search)throw new ApiError(404,'NOT_FOUND','Workflow action not found.');const actor=await requireWorkContext(holdingSlug);return Response.json({data:await salesServiceProducts(actor)},{headers:{'Cache-Control':'private, no-store'}});}if(path[0]==='sales-goods-posting-setup'){const actor=await requireWorkContext(holdingSlug);if(path.length!==1)throw new ApiError(404,'NOT_FOUND','Workflow action not found.');return Response.json({data:await salesGoodsPostingSetup(actor)},{headers:{'Cache-Control':'private, no-store'}});}if(path[0]==='sales-posting-setup'){const actor=await requireWorkContext(holdingSlug);if(path.length!==1)throw new ApiError(404,'NOT_FOUND','Workflow action not found.');return Response.json({data:await salesPostingSetup(actor)},{headers:{'Cache-Control':'private, no-store'}});}if(path[0]==='posting-setup'){
    const actor=await requireWorkContext(holdingSlug);if(path.length===2&&path[1]==='prerequisites')return Response.json({data:await supplyPostingPrerequisites(actor)},{headers:{'Cache-Control':'private, no-store'}});if(path.length!==1)throw new ApiError(404,'NOT_FOUND','Workflow action not found.');
    return Response.json({data:await commercePostingSetup(actor)},{headers:{'Cache-Control':'private, no-store'}});
  }}catch(e){return errorResponse(e);}
  try{const {holdingSlug,path}=await context.params;const actor=await requireWorkContext(holdingSlug);return Response.json(canonicalDestinationLinks({data:path[0]==='pricing'?await pricingWorkspace(actor):path[0]==="orders"&&path[1]?await getOrder(actor,path[1]):path[0]==="reorder-policies"?await listInventoryReorderPolicies(actor):await commerceOverview(actor)}),{headers:{"Cache-Control":"private, no-store"}});}catch(e){return errorResponse(e);}
}
export async function POST(request:Request,context:Context) {
  try{
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin for this action.");
    const {holdingSlug,path:p}=await context.params;const actor=await requireWorkContext(holdingSlug);const key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");
    const input=await request.json();await assertPilotCommercePath(actor,p,input);let data;
    if(p[0]==='sales-service-products'&&p.length===1){if(new URL(request.url).search)throw new ApiError(404,'NOT_FOUND','Workflow action not found.');data=await createSalesServiceProduct(actor,salesServiceProductSchema.parse(input),key);}
    else if(p[0]==='posting-setup'&&p.length===1)data=await initializeCommercePostingSetup(actor,commercePostingSetupSchema.parse(input),key);
    else if(p[0]==='posting-setup'&&p.length===2&&p[1]==='charts')data=await createSupplyPostingChart(actor,supplyPostingChartSchema.parse(input),key);
    else if(p[0]==='posting-setup'&&p.length===2&&p[1]==='accounts')data=await createSupplyPostingAccount(actor,supplyPostingAccountSchema.parse(input),key);
    else if(p[0]==='posting-setup'&&p.length===2&&p[1]==='periods')data=await createSupplyPostingPeriod(actor,supplyPostingPeriodSchema.parse(input),key);
    else if(p[0]==='posting-setup'&&p.length===2&&p[1]==='rates')data=await confirmSupplyPostingRate(actor,supplyPostingRateSchema.parse(input),key);
    else if(p[0]==='sales-goods-posting-setup'&&p.length===1)data=await initializeSalesGoodsPostingSetup(actor,salesGoodsPostingSetupSchema.parse(input),key);
    else if(p[0]==='sales-posting-setup'&&p.length===2&&p[1]==='charts')data=await createSalesPostingChart(actor,salesPostingChartSchema.parse(input),key);
    else if(p[0]==='sales-posting-setup'&&p.length===2&&p[1]==='accounts')data=await createSalesPostingAccount(actor,salesPostingAccountSchema.parse(input),key);
    else if(p[0]==='sales-posting-setup'&&p.length===2&&p[1]==='periods')data=await createSalesPostingPeriod(actor,salesPostingPeriodSchema.parse(input),key);
    else if(p[0]==='sales-posting-setup'&&p.length===2&&p[1]==='rates')data=await confirmSalesPostingRate(actor,salesPostingRateSchema.parse(input),key);
    else if(p[0]==='sales-posting-setup'&&p.length===2&&p[1]==='defaults')data=await prepareSalesPostingDefaults(actor,salesPostingDefaultsSchema.parse(input),key);
    else if(p[0]==='sales-posting-setup'&&p.length===4&&p[1]==='defaults'&&p[3]==='decide')data=await decideSalesPostingDefaults(actor,p[2],salesPostingDecisionSchema.parse(input),key);
    else if(p[0]==='reorder-policies'&&p.length===1)data=await saveInventoryReorderPolicy(actor,inventoryReorderPolicySchema.parse(input),key);
    else if(p[0]==='reorder-policies'&&p.length===3&&["approve","reject","withdraw","retire"].includes(p[2]))data=await transitionInventoryReorderPolicy(actor,p[1],inventoryReorderPolicyActionSchema.parse({...input,action:p[2]}),key);
    else if(p[0]==='pricing'&&p.length===1)data=await nativePricing(actor,'create',null,input,key,request.headers);
    else if(p[0]==='pricing'&&p.length===3&&p[2]==='edit')data=await nativePricing(actor,'update',p[1],input,key,request.headers);
    else if(p[0]==='pricing'&&p.length===2)data=await nativePricing(actor,'decide',p[1],input,key,request.headers);
    else if(p[0]==="orders"&&p.length<=2){const order=orderSchema.parse(input);const kind=order.kind;if(kind==="purchase")data=await saveOrder(actor,p[1]??null,order,key);else{const nativeOrder={...order,kind};data=(await nativeCommerceOrderSave(actor,p[1]??null,nativeOrder,key))??await saveOrder(actor,p[1]??null,order,key);}}
    else if(p[0]==="orders"&&p[2]==="transition"){const transition=transitionSchema.parse(input);const cancelled=transition.action==="cancel"?await nativeSalesConfirmedCancellation(actor,p[1],transition.expectedVersion,key):undefined;data=cancelled??(await nativeCommerceOrderTransition(actor,p[1],transition,key))??await transitionOrder(actor,p[1],transition,key);}
    else if(p[0]==="orders"&&p[2]==="reservations")data=await changeOrderReservation(actor,p[1],input,key);
    else if(p[0]==="orders"&&p[2]==="fulfill"){const f=fulfillmentSchema.parse(input);const lines=(line:any)=>line.allocations?.length?{allocations:line.allocations.map((a:any)=>({...(a.lotId?{lotId:a.lotId}:{}),...(a.lotCode?{lotCode:a.lotCode}:{}),...(a.serialNumber?{serialNumber:a.serialNumber}:{}),quantity:a.quantity,...(a.disposition?{disposition:a.disposition}:{}),...(a.quarantineReason?{quarantineReason:a.quarantineReason}:{}),...(a.quarantineDueDate?{quarantineDueDate:a.quarantineDueDate}:{})}))}:{};if(f.completionMode==="service"){const native={expectedVersion:f.expectedVersion,date:f.date,evidenceNote:f.evidenceNote,lines:f.lines.map(l=>({lineId:l.lineId,quantity:l.quantity}))};data=await nativeCommerceOrderFulfill(actor,p[1],native,key);if(!data)throw new ApiError(503,'NATIVE_COMMERCE_ORDER_UNAVAILABLE','The native Commerce service completion is unavailable.');}else{const deliveryLines=f.lines.map((l:any)=>({lineId:l.lineId,quantity:l.quantity,...lines(l)}));const purchaseLines=f.lines.map((l:any)=>({lineId:l.lineId,quantity:l.quantity,...(l.allocations.length?{allocations:l.allocations.map((a:any)=>({...(a.lotCode?{lotCode:a.lotCode}:{}),...(a.serialNumber?{serialNumber:a.serialNumber}:{}),quantity:a.quantity,...(a.manufacturedDate?{manufacturedDate:a.manufacturedDate}:{}),...(a.expiryDate?{expiryDate:a.expiryDate}:{}),...(a.disposition?{disposition:a.disposition}:{}),...(a.quarantineReason?{quarantineReason:a.quarantineReason}:{}),...(a.quarantineDueDate?{quarantineDueDate:a.quarantineDueDate}:{})}))}:{})}));const receipt=f.completionMode==="goods"?await nativeCommerceGoodsReceipt(actor,p[1],{expectedVersion:f.expectedVersion,date:f.date,lines:purchaseLines},key):undefined;data=receipt??await nativeCommerceSalesDelivery(actor,p[1],{completionMode:f.completionMode==="mixed"?"mixed":"goods",expectedVersion:f.expectedVersion,date:f.date,lines:deliveryLines},key);}}
    else if(p[0]==="fulfillments"&&p[2]==="invoice")data=await invoiceFulfillment(actor,p[1],invoiceFromDeliverySchema.parse(input),key);
    else if(p[0]==='fulfillments'&&p[2]==='notify')data=await nativeQueueDeliveryNotification(actor,p[1],input,key,request.headers);
    else if(p[0]==='customer-notifications'&&p[1]==='settings')data=await nativeUpdateCustomerNotificationSettings(actor,input,key,request.headers);
    else if(p[0]==='customer-notifications'&&p[2]==='reconcile')data=await nativeSalesNotificationReconcile(actor,p[1],salesNotificationReconciliationInput(input),key,request.headers);
    else if(p[0]==='customer-notifications'&&p[2]==='retry')data=await nativeRetryCustomerNotification(actor,p[1],input,key,request.headers);
    else if(p[0]==="invoices"&&p[2]==="settle")data=await settleInvoice(actor,p[1],settlementSchema.parse(input),key);
    else if(p[0]==="fulfillments"&&p[2]==="returns")data=await requestCommerceReturn(actor,p[1],returnRequestSchema.parse(input),key);
    else if(p[0]==="returns"&&p[2]==="approve")data=await decideCommerceReturn(actor,p[1],"approve",returnDecisionSchema.parse(input),key);
    else if(p[0]==="returns"&&p[2]==="reject")data=await decideCommerceReturn(actor,p[1],"reject",returnDecisionSchema.parse(input),key);
    else if(p[0]==="returns"&&p[2]==="complete")data=await completeCommerceReturn(actor,p[1],returnCompleteSchema.parse(input),key);
    else if(["products","parties","warehouses"].includes(p[0])&&p.length<=2)data=await saveMaster(actor,p[0] as "products"|"parties"|"warehouses",p[1]??null,input,key);
    else throw new ApiError(404,"NOT_FOUND","Workflow action not found.");
    return Response.json(canonicalDestinationLinks({data}),{headers:['posting-setup','sales-posting-setup','sales-goods-posting-setup','sales-service-products'].includes(p[0])?{'Cache-Control':'private, no-store'}:undefined});
  }catch(e){if(e instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",e.issues[0]?.message??"Invalid input."));if(e instanceof SyntaxError)return errorResponse(new ApiError(422,"INVALID_JSON","Use valid JSON."));return errorResponse(e);}
}
