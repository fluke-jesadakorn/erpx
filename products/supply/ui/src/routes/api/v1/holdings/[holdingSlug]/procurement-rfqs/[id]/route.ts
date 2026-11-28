import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {requestPublicOrigin} from "@erpx/manage-erpx-server/public/public-origin";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {actRfq,rfqDetail} from "@erpx/procurement-server/public/procurement-rfqs";
type Context={params:Promise<{holdingSlug:string;id:string}>};
export async function GET(_r:Request,c:Context){try{const {holdingSlug,id}=await c.params;return Response.json(canonicalDestinationLinks({data:await rfqDetail(await requireWorkContext(holdingSlug),id)}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const {holdingSlug,id}=await c.params;return Response.json(canonicalDestinationLinks({data:await actRfq(await requireWorkContext(holdingSlug),id,await r.json(),key)}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
