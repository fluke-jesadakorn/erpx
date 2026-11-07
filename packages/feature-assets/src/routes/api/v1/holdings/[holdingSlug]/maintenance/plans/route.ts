import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {changePreventivePlan,savePreventivePlan} from "@erpx/server-maintenance/public/maintenance-preventive";
import {ownedMaintenancePlanDetail,ownedMaintenancePlans} from "@erpx/feature-assets/adapters/server/native-maintenance-owned";
type C={params:Promise<{holdingSlug:string}>};
const headers={'Cache-Control':'private, no-store'};
function unavailable():never{throw new ApiError(503,'NATIVE_MAINTENANCE_UNAVAILABLE','The native Maintenance service is unavailable.');}
export async function GET(request:Request,context:C){
 try{
  const actor=await requireWorkContext((await context.params).holdingSlug),id=new URL(request.url).searchParams.get('id');
  const data=id?await ownedMaintenancePlanDetail(actor,actor.holdingSlug,id):await ownedMaintenancePlans(actor,actor.holdingSlug);
  if(data===undefined)unavailable();
  return Response.json(canonicalDestinationLinks({data}),{headers});
 }catch(error){return errorResponse(error);}
}
/** Preventive plan writes remain on the retained TypeScript application until
 * the native plan command composition is mounted. Reads above are native. */
export async function POST(request:Request,context:C){
 try{
  if(request.headers.get('origin')&&request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
  const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
  const actor=await requireWorkContext((await context.params).holdingSlug),input=await request.json();
  if(!input||typeof input!=='object'||Array.isArray(input))throw new ApiError(422,'PLAN_INPUT_INVALID','Use a preventive plan command object.');
  const id=typeof input.id==='string'?input.id:null;
  return Response.json(canonicalDestinationLinks({data:id&&input.action?await changePreventivePlan(actor,id,input,key):await savePreventivePlan(actor,id,input,key)}),{headers});
 }catch(error){return errorResponse(error);}
}
