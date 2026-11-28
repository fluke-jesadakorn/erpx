import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { approveBookingRefund } from "@erpx/pms-server/public/pms-booking";
import { retryBookingRefund,recoverBookingReceipt } from "@erpx/pms-server/public/pms-booking-refunds";
import { ApiError,errorResponse } from "@erpx/platform-runtime-server/public/http";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import { holdingResources } from "../../../../../../../../../../../platform/runtime/server/src/public/runtime";
import { isTransactionalSqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";

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
  return Response.json(canonicalDestinationLinks({data:body.action==='recover-receipt'?await recoverBookingReceipt(write,actor,body.id,reason):body.action==='retry'?await retryBookingRefund(write,actor,body.id,reason,request.headers):await approveBookingRefund(write,actor,body.id)}),{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
