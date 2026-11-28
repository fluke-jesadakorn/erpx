import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {requestPublicOrigin} from "@erpx/manage-erpx-server/public/public-origin";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {headers} from "next/headers";
import {customerFollowupWorkspace} from "@erpx/sales-server/public/customer-followups";
import {nativeCreateCustomerFollowup,nativeChangeCustomerFollowup} from "@erpx/sales-ui/public/native-sales-followups";
type Context={params:Promise<{holdingSlug:string}>};const responseHeaders={'Cache-Control':'private, no-store'};
export async function GET(r:Request,c:Context){try{return Response.json(canonicalDestinationLinks({data:await customerFollowupWorkspace(await requireWorkContext((await c.params).holdingSlug),new URL(r.url).searchParams.get('orderId')??undefined)}),{headers:responseHeaders});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();return Response.json(canonicalDestinationLinks({data:typeof input.id==='string'?await nativeChangeCustomerFollowup(a,input.id,input,key,await headers()):await nativeCreateCustomerFollowup(a,input,key,await headers())}),{headers:responseHeaders});}catch(e){return errorResponse(e);}}
