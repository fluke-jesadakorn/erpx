import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {requestPublicOrigin} from "@erpx/manage-erpx-server/public/public-origin";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {createRfq,rfqWorkspace} from "@erpx/procurement-server/public/procurement-rfqs";
type Context={params:Promise<{holdingSlug:string}>};
export async function GET(_r:Request,c:Context){try{return Response.json(canonicalDestinationLinks({data:await rfqWorkspace(await requireWorkContext((await c.params).holdingSlug))}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');return Response.json(canonicalDestinationLinks({data:await createRfq(await requireWorkContext((await c.params).holdingSlug),await r.json(),key)}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
