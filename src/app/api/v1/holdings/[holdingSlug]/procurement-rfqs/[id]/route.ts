import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {actRfq,rfqDetail} from "@/server/modules/procurement/public/procurement-rfqs";
type Context={params:Promise<{holdingSlug:string;id:string}>};
export async function GET(_r:Request,c:Context){try{const {holdingSlug,id}=await c.params;return Response.json({data:await rfqDetail(await requireWorkContext(holdingSlug),id)},{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const {holdingSlug,id}=await c.params;return Response.json({data:await actRfq(await requireWorkContext(holdingSlug),id,await r.json(),key)},{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
