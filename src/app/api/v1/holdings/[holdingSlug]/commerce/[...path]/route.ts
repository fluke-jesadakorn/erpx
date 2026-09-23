import { changeOrderReservation } from "@/server/commerce";
import { assertProductionModuleEnabled } from '@/server/hospitality-rollout';
import { holdingResources } from '@/server/runtime';
import { requestPublicOrigin } from "@/server/public-origin";
import { z } from "zod";
import { requireWorkContext } from "@/server/authorization";
import { ApiError,errorResponse } from "@/server/http";
import { commerceOverview,getOrder,saveOrder,transitionOrder,fulfillOrder,invoiceFulfillment,settleInvoice,saveMaster,requestCommerceReturn,decideCommerceReturn,completeCommerceReturn,orderSchema,transitionSchema,fulfillmentSchema,invoiceFromDeliverySchema,settlementSchema,returnRequestSchema,returnDecisionSchema,returnCompleteSchema } from "@/server/commerce";
import {decidePriceList,priceListDecisionSchema,priceListSchema,priceListUpdateSchema,pricingWorkspace,savePriceList,updatePriceList} from '@/server/sales-pricing';
import {customerNotificationRetrySchema,customerNotificationSchema,customerNotificationSettingsSchema,queueDeliveryNotification,retryCustomerNotification,updateCustomerNotificationSettings} from '@/server/sales-customer-notifications';
type Context={params:Promise<{holdingSlug:string;path:string[]}>};
async function assertPilotCommercePath(actor:Awaited<ReturnType<typeof requireWorkContext>>,path:string[],input:unknown){
  if(process.env.NODE_ENV!=='production')return;
  const db=(await holdingResources(actor.holdingId)).write;
  const root=path[0];
  if(root==='products'||root==='warehouses')return assertProductionModuleEnabled(db,actor.companyId,'inventory');
  if(root==='pricing'||root==='customer-notifications')return assertProductionModuleEnabled(db,actor.companyId,'sales');
  if(root==='parties'){
    const existing=path[1]?await db.prepare('SELECT kind FROM finance_parties WHERE id=? AND company_id=?').bind(path[1],actor.companyId).first<{kind:string}>():null;
    const kind=existing?.kind??(input as {kind?:string})?.kind;
    return assertProductionModuleEnabled(db,actor.companyId,kind==='supplier'?'procurement':'sales');
  }
  let kind:string|undefined;
  if(root==='orders'){
    kind=path[1]&&path.length>2?(await db.prepare('SELECT kind FROM commerce_orders WHERE id=? AND company_id=?').bind(path[1],actor.companyId).first<{kind:string}>())?.kind:(input as {kind?:string})?.kind;
  }else if(root==='fulfillments'){
    kind=(await db.prepare('SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=?').bind(path[1],actor.companyId).first<{kind:string}>())?.kind;
  }else if(root==='invoices'){
    kind=(await db.prepare('SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.invoice_id=? AND o.company_id=?').bind(path[1],actor.companyId).first<{kind:string}>())?.kind;
  }else if(root==='returns'){
    kind=(await db.prepare('SELECT o.kind FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id WHERE r.id=? AND o.company_id=?').bind(path[1],actor.companyId).first<{kind:string}>())?.kind;
  }
  return assertProductionModuleEnabled(db,actor.companyId,kind==='purchase'?'procurement':'sales');
}
export async function GET(_request:Request,context:Context) {
  try{const {holdingSlug,path}=await context.params;const actor=await requireWorkContext(holdingSlug);return Response.json({data:path[0]==='pricing'?await pricingWorkspace(actor):path[0]==="orders"&&path[1]?await getOrder(actor,path[1]):await commerceOverview(actor)},{headers:{"Cache-Control":"private, no-store"}});}catch(e){return errorResponse(e);}
}
export async function POST(request:Request,context:Context) {
  try{
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin for this action.");
    const {holdingSlug,path:p}=await context.params;const actor=await requireWorkContext(holdingSlug);const key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");
    const input=await request.json();await assertPilotCommercePath(actor,p,input);let data;
    if(p[0]==='pricing'&&p.length===1)data=await savePriceList(actor,priceListSchema.parse(input),key);
    else if(p[0]==='pricing'&&p.length===3&&p[2]==='edit')data=await updatePriceList(actor,p[1],priceListUpdateSchema.parse(input),key);
    else if(p[0]==='pricing'&&p.length===2)data=await decidePriceList(actor,p[1],priceListDecisionSchema.parse(input),key);
    else if(p[0]==="orders"&&p.length<=2)data=await saveOrder(actor,p[1]??null,orderSchema.parse(input),key);
    else if(p[0]==="orders"&&p[2]==="transition")data=await transitionOrder(actor,p[1],transitionSchema.parse(input),key);
    else if(p[0]==="orders"&&p[2]==="reservations")data=await changeOrderReservation(actor,p[1],input,key);
    else if(p[0]==="orders"&&p[2]==="fulfill")data=await fulfillOrder(actor,p[1],fulfillmentSchema.parse(input),key);
    else if(p[0]==="fulfillments"&&p[2]==="invoice")data=await invoiceFulfillment(actor,p[1],invoiceFromDeliverySchema.parse(input),key);
    else if(p[0]==='fulfillments'&&p[2]==='notify')data=await queueDeliveryNotification(actor,p[1],customerNotificationSchema.parse(input),key);
    else if(p[0]==='customer-notifications'&&p[1]==='settings')data=await updateCustomerNotificationSettings(actor,customerNotificationSettingsSchema.parse(input),key);
    else if(p[0]==='customer-notifications'&&p[2]==='retry')data=await retryCustomerNotification(actor,p[1],customerNotificationRetrySchema.parse(input),key);
    else if(p[0]==="invoices"&&p[2]==="settle")data=await settleInvoice(actor,p[1],settlementSchema.parse(input),key);
    else if(p[0]==="fulfillments"&&p[2]==="returns")data=await requestCommerceReturn(actor,p[1],returnRequestSchema.parse(input),key);
    else if(p[0]==="returns"&&p[2]==="approve")data=await decideCommerceReturn(actor,p[1],"approve",returnDecisionSchema.parse(input),key);
    else if(p[0]==="returns"&&p[2]==="reject")data=await decideCommerceReturn(actor,p[1],"reject",returnDecisionSchema.parse(input),key);
    else if(p[0]==="returns"&&p[2]==="complete")data=await completeCommerceReturn(actor,p[1],returnCompleteSchema.parse(input),key);
    else if(["products","parties","warehouses"].includes(p[0])&&p.length<=2)data=await saveMaster(actor,p[0] as "products"|"parties"|"warehouses",p[1]??null,input,key);
    else throw new ApiError(404,"NOT_FOUND","Workflow action not found.");
    return Response.json({data});
  }catch(e){if(e instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",e.issues[0]?.message??"Invalid input."));if(e instanceof SyntaxError)return errorResponse(new ApiError(422,"INVALID_JSON","Use valid JSON."));return errorResponse(e);}
}
