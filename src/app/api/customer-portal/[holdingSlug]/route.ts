import {boundedBody} from "@/server/modules/legal/public/legal/body";
import {customerPortalCommand,customerPortalContext,customerPortalList,customerPortalProjection} from "@/server/modules/customer_360/public/customer-portal";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import type {PostgresDatabase} from "@/server/modules/runtime/public/runtime/postgres";
export const dynamic='force-dynamic';
type Context={params:Promise<{holdingSlug:string}>};
const headers={'Cache-Control':'private, no-store'};
export async function GET(_request:Request,ctx:Context){try{return Response.json({data:await customerPortalList((await ctx.params).holdingSlug)},{headers});}catch(e){return errorResponse(e);}}
export async function POST(request:Request,ctx:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
 const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','A command key is required.');
 let raw:unknown;try{raw=JSON.parse(new TextDecoder().decode(await boundedBody(request,50000)));}catch(error){if(error instanceof SyntaxError)throw new ApiError(422,'INVALID_JSON','Invalid JSON.');throw error;}
 const {holdingSlug}=await ctx.params,{write,actor}=await customerPortalContext(holdingSlug);
 await customerPortalProjection(write,actor);
 return Response.json({data:await customerPortalCommand(write as PostgresDatabase,actor,raw,key)},{headers});
}catch(e){return errorResponse(e);}}
