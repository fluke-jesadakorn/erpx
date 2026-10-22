import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {changePreventivePlan,preventivePlanDetail,preventivePlans,savePreventivePlan} from "@erpx/server-maintenance/public/maintenance-preventive";
type Context={params:Promise<{holdingSlug:string}>};
const headers={'Cache-Control':'private, no-store'};
export async function GET(request:Request,context:Context){try{const a=await requireWorkContext((await context.params).holdingSlug),id=new URL(request.url).searchParams.get('id');return Response.json(canonicalDestinationLinks({data:id?await preventivePlanDetail(a,id):await preventivePlans(a)}),{headers});}catch(error){return errorResponse(error);}}
export async function POST(request:Request,context:Context){try{
 if(request.headers.get('origin')&&request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
 const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
 const actor=await requireWorkContext((await context.params).holdingSlug),input=await request.json();
 if(!input||typeof input!=='object'||Array.isArray(input))throw new ApiError(422,'PLAN_INPUT_INVALID','Use a preventive plan command object.');
 const id=typeof input.id==='string'?input.id:null;
 return Response.json(canonicalDestinationLinks({data:id&&input.action?await changePreventivePlan(actor,id,input,key):await savePreventivePlan(actor,id,input,key)}),{headers});
}catch(error){return errorResponse(error);}}
