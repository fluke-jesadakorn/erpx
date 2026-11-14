import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {getLandedCost,landedCostActionSchema,landedCostCreateSchema,landedCostDetail} from "@erpx/server-inventory/public/inventory-landed-costs";
import {nativeLandedCostCommand} from "@erpx/feature-supply/lib/native-inventory-command";
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
export async function GET(_request:Request,context:Context){try{const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug);if(path.length!==1)throw new ApiError(404,'NOT_FOUND','Landed cost route not found.');return Response.json(canonicalDestinationLinks({data:await landedCostDetail(actor,path[0])}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}}
export async function POST(request:Request,context:Context){try{const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const {holdingSlug,path=[]}=await context.params;await requireWorkContext(holdingSlug);const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=await request.json();let native;
 if(!path.length){const parsed=landedCostCreateSchema.parse(input);native=await nativeLandedCostCommand(request,holdingSlug,undefined,undefined,parsed);}
 else if(path.length===2&&['approve','reject','cancel','post'].includes(path[1])){const parsed=landedCostActionSchema.parse(input);native=await nativeLandedCostCommand(request,holdingSlug,path[0],path[1],parsed);}
 else throw new ApiError(404,'NOT_FOUND','Landed cost action not found.');
 if(native)return native;
 throw new ApiError(503,'NATIVE_INVENTORY_LANDED_COST_UNAVAILABLE','The native landed-cost command is unavailable.');
}catch(error){return errorResponse(error);}}
