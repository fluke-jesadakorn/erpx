import {requireWorkContext} from '@/server/authorization';
import {holdingResources} from '@/server/runtime';
import {isTransactionalSqlDatabase} from '@/server/runtime/types';
import {requestPublicOrigin} from '@/server/public-origin';
import {ApiError,errorResponse} from '@/server/http';
import {staffBookingChange} from '@/server/pms/staff-booking-changes';
export const dynamic='force-dynamic';
export async function POST(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{
  if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the ERPX website');
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
  if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
  return Response.json({data:await staffBookingChange(write,actor,holdingSlug,await request.json(),request.headers.get('Idempotency-Key')??'')},{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
