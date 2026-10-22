import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {boundedBody} from "@erpx/server-legal/public/legal/body";
import {customerPortalProjectionRequest} from "@erpx/server-customer_360/public/portal-projection-request";
import {customerPortalCommandRequest} from "@erpx/server-customer_360/public/portal-command-request";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
export const dynamic='force-dynamic';
type Context={params:Promise<{holdingSlug:string}>};
const headers={'Cache-Control':'private, no-store'};
export async function GET(_request:Request,ctx:Context){try{return Response.json(canonicalDestinationLinks({data:await customerPortalProjectionRequest((await ctx.params).holdingSlug)}),{headers});}catch(e){return errorResponse(e);}}
export async function POST(request:Request,ctx:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
 const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','A command key is required.');
 let raw:unknown;try{raw=JSON.parse(new TextDecoder().decode(await boundedBody(request,50000)));}catch(error){if(error instanceof SyntaxError)throw new ApiError(422,'INVALID_JSON','Invalid JSON.');throw error;}
 const {holdingSlug}=await ctx.params;
 return Response.json(canonicalDestinationLinks({data:await customerPortalCommandRequest(holdingSlug,raw,key)}),{headers});
}catch(e){return errorResponse(e);}}
