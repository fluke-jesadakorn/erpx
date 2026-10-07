import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {isTransactionalSqlDatabase} from "@/server/modules/runtime/public/runtime/types";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {staffBookingChange} from "@/server/modules/pms/public/pms/staff-booking-changes";
export const dynamic='force-dynamic';
export async function POST(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{
  if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the ERPX website');
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
  if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
  return Response.json({data:await staffBookingChange(write,actor,holdingSlug,await request.json(),request.headers.get('Idempotency-Key')??'')},{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
