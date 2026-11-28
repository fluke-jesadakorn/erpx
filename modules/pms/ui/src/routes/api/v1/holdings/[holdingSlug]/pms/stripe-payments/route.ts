import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {createPmsStripePayment} from "@erpx/pms-server/public/pms-stripe";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {requestPublicOrigin} from "@erpx/manage-erpx-server/public/public-origin";
import {holdingResources} from "../../../../../../../../../../../platform/runtime/server/src/public/runtime";
import {isTransactionalSqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";

type Context={params:Promise<{holdingSlug:string}>};
export async function POST(request:Request,{params}:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the ERPX website');
 const actor=await requireWorkContext((await params).holdingSlug),key=request.headers.get('Idempotency-Key');
 if(!key)throw new ApiError(422,'IDEMPOTENCY_KEY_REQUIRED','Provide a payment request key');
 const {write}=await holdingResources(actor.holdingId);if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
 return Response.json(canonicalDestinationLinks({data:await createPmsStripePayment(write,actor,await request.json(),key)}),{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
