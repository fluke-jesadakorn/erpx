import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {holdingResources} from "../../../../../../../../../../../platform/runtime/server/src/public/runtime";
import {isTransactionalSqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {requestPublicOrigin} from "@erpx/manage-erpx-server/public/public-origin";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {staffBookingChange} from "@erpx/pms-server/public/pms/staff-booking-changes";
export const dynamic='force-dynamic';
export async function POST(request:Request,{params}:{params:Promise<{holdingSlug:string}>}){
 try{
  if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the ERPX website');
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
  if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
  return Response.json(canonicalDestinationLinks({data:await staffBookingChange(write,actor,holdingSlug,await request.json(),request.headers.get('Idempotency-Key')??'')}),{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){return errorResponse(error);}
}
