import {salesServiceProducts,createSalesServiceProduct,salesServiceProductSchema} from '@/server/modules/commerce/public/sales-service-products';
import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import {commerceEntrypointFacts} from "@/server/modules/commerce/public/entrypoint-facts";
import {commerceEntrypointFinanceFacts} from "@/server/modules/finance/public/commerce-entrypoint-facts";
import { changeOrderReservation } from "@/server/modules/commerce/public/commerce";
import { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { z } from "zod";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError,errorResponse } from "@/server/modules/runtime/public/http";
import { commerceOverview,getOrder,saveOrder,transitionOrder,fulfillOrder,invoiceFulfillment,settleInvoice,saveMaster,requestCommerceReturn,decideCommerceReturn,completeCommerceReturn,orderSchema,transitionSchema,fulfillmentSchema,invoiceFromDeliverySchema,settlementSchema,returnRequestSchema,returnDecisionSchema,returnCompleteSchema } from "@/server/modules/commerce/public/commerce";
import {decidePriceList,priceListDecisionSchema,priceListSchema,priceListUpdateSchema,pricingWorkspace,savePriceList,updatePriceList} from "@/server/modules/sales/public/sales-pricing";
import {customerNotificationReconciliationSchema,reconcileCustomerNotification,customerNotificationRetrySchema,customerNotificationSchema,customerNotificationSettingsSchema,queueDeliveryNotification,retryCustomerNotification,updateCustomerNotificationSettings} from "@/server/modules/sales/public/sales-customer-notifications";
import { inventoryReorderPolicyActionSchema, inventoryReorderPolicySchema, listInventoryReorderPolicies, saveInventoryReorderPolicy, transitionInventoryReorderPolicy } from "@/server/modules/inventory/public/inventory-reorder-policies";
import {commercePostingSetup,initializeCommercePostingSetup,commercePostingSetupSchema} from "@/server/modules/commerce/public/posting-setup";
import {supplyPostingPrerequisites,createSupplyPostingChart,createSupplyPostingAccount,createSupplyPostingPeriod,confirmSupplyPostingRate,supplyPostingChartSchema,supplyPostingAccountSchema,supplyPostingPeriodSchema,supplyPostingRateSchema} from "@/server/modules/finance/public/supply-posting-prerequisites";
import {salesPostingSetup,createSalesPostingChart,createSalesPostingAccount,createSalesPostingPeriod,confirmSalesPostingRate,prepareSalesPostingDefaults,decideSalesPostingDefaults,salesPostingChartSchema,salesPostingAccountSchema,salesPostingPeriodSchema,salesPostingRateSchema,salesPostingDefaultsSchema,salesPostingDecisionSchema} from "@/server/modules/finance/public/sales-posting-setup";
import {salesGoodsPostingSetup,initializeSalesGoodsPostingSetup,salesGoodsPostingSetupSchema} from "@/server/modules/commerce/public/sales-goods-posting-setup";
type Context={params:Promise<{holdingSlug:string;path:string[]}>};
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
    else if(p[0]==='pricing'&&p.length===1)data=await savePriceList(actor,priceListSchema.parse(input),key);
    else if(p[0]==='pricing'&&p.length===3&&p[2]==='edit')data=await updatePriceList(actor,p[1],priceListUpdateSchema.parse(input),key);
    else if(p[0]==='pricing'&&p.length===2)data=await decidePriceList(actor,p[1],priceListDecisionSchema.parse(input),key);
    else if(p[0]==="orders"&&p.length<=2)data=await saveOrder(actor,p[1]??null,orderSchema.parse(input),key);
    else if(p[0]==="orders"&&p[2]==="transition")data=await transitionOrder(actor,p[1],transitionSchema.parse(input),key);
    else if(p[0]==="orders"&&p[2]==="reservations")data=await changeOrderReservation(actor,p[1],input,key);
    else if(p[0]==="orders"&&p[2]==="fulfill")data=await fulfillOrder(actor,p[1],fulfillmentSchema.parse(input),key);
    else if(p[0]==="fulfillments"&&p[2]==="invoice")data=await invoiceFulfillment(actor,p[1],invoiceFromDeliverySchema.parse(input),key);
    else if(p[0]==='fulfillments'&&p[2]==='notify')data=await queueDeliveryNotification(actor,p[1],customerNotificationSchema.parse(input),key);
    else if(p[0]==='customer-notifications'&&p[1]==='settings')data=await updateCustomerNotificationSettings(actor,customerNotificationSettingsSchema.parse(input),key);
    else if(p[0]==='customer-notifications'&&p[2]==='reconcile')data=await reconcileCustomerNotification(actor,p[1],customerNotificationReconciliationSchema.parse(input),key);
    else if(p[0]==='customer-notifications'&&p[2]==='retry')data=await retryCustomerNotification(actor,p[1],customerNotificationRetrySchema.parse(input),key);
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
