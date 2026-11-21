import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {nativeCustomerDuplicateCommand,nativeCustomerDuplicates} from "@erpx/server-customer_authority/public/native-customer-authority";
type Context={params:Promise<{holdingSlug:string}>};const headers={'Cache-Control':'private, no-store'};
export async function GET(r:Request,c:Context){try{const holdingSlug=(await c.params).holdingSlug,id=new URL(r.url).searchParams.get('id');return Response.json(canonicalDestinationLinks({data:await nativeCustomerDuplicates(r,holdingSlug,id)}),{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{const holdingSlug=(await c.params).holdingSlug,key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=await r.json();return Response.json(canonicalDestinationLinks({data:await nativeCustomerDuplicateCommand(r,holdingSlug,input,key)}),{headers});}catch(e){return errorResponse(e);}}
