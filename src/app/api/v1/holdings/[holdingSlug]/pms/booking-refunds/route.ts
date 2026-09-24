import { requireWorkContext } from '@/server/authorization';
import { approveBookingRefund } from '@/server/pms-booking';
import { ApiError,errorResponse } from '@/server/http';
import { requestPublicOrigin } from '@/server/public-origin';
import { holdingResources } from '@/server/runtime';
import { isTransactionalSqlDatabase } from '@/server/runtime/types';

type Context={params:Promise<{holdingSlug:string}>};
export async function POST(request:Request,{params}:Context){
 try{
  if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the ERPX website');
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug);
  const body=await request.json();
  if(typeof body?.id!=='string')throw new ApiError(422,'PMS_REFUND_ID_REQUIRED','Select a refund request');
  const {write}=await holdingResources(actor.holdingId);
  if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
  return Response.json({data:await approveBookingRefund(write,actor,body.id)},{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
