import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { approveBookingRefund } from "@/server/modules/pms/public/pms-booking";
import { retryBookingRefund,recoverBookingReceipt } from "@/server/modules/pms/public/pms-booking-refunds";
import { ApiError,errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";

type Context={params:Promise<{holdingSlug:string}>};
export async function POST(request:Request,{params}:Context){
 try{
  if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the ERPX website');
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug);
  const body=await request.json();
  if(typeof body?.id!=='string')throw new ApiError(422,'PMS_REFUND_ID_REQUIRED','Select a refund request');
  const {write}=await holdingResources(actor.holdingId);
  if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
  if(body.action && !['approve','retry','recover-receipt'].includes(body.action))throw new ApiError(422,'PMS_REFUND_ACTION','Choose an available refund action');
  const reason=typeof body.reason==='string'?body.reason:'';
  return Response.json({data:body.action==='recover-receipt'?await recoverBookingReceipt(write,actor,body.id,reason):body.action==='retry'?await retryBookingRefund(write,actor,body.id,reason):await approveBookingRefund(write,actor,body.id)},{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
