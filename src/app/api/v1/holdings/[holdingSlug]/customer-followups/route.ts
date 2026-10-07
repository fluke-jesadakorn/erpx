import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {customerFollowupWorkspace,createCustomerFollowup,changeCustomerFollowup} from "@/server/modules/sales/public/customer-followups";
type Context={params:Promise<{holdingSlug:string}>};const headers={'Cache-Control':'private, no-store'};
export async function GET(r:Request,c:Context){try{return Response.json({data:await customerFollowupWorkspace(await requireWorkContext((await c.params).holdingSlug),new URL(r.url).searchParams.get('orderId')??undefined)},{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();return Response.json({data:typeof input.id==='string'?await changeCustomerFollowup(a,input.id,input,key):await createCustomerFollowup(a,input,key)},{headers});}catch(e){return errorResponse(e);}}
