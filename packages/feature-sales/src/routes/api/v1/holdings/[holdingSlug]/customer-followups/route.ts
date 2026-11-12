import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {headers} from "next/headers";
import {customerFollowupWorkspace} from "@erpx/server-sales/public/customer-followups";
import {nativeCreateCustomerFollowup,nativeChangeCustomerFollowup} from "@erpx/feature-sales/public/native-sales-followups";
type Context={params:Promise<{holdingSlug:string}>};const headers={'Cache-Control':'private, no-store'};
export async function GET(r:Request,c:Context){try{return Response.json(canonicalDestinationLinks({data:await customerFollowupWorkspace(await requireWorkContext((await c.params).holdingSlug),new URL(r.url).searchParams.get('orderId')??undefined)}),{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();return Response.json(canonicalDestinationLinks({data:typeof input.id==='string'?await nativeChangeCustomerFollowup(a,input.id,input,key,await headers()):await nativeCreateCustomerFollowup(a,input,key,await headers())}),{headers});}catch(e){return errorResponse(e);}}
