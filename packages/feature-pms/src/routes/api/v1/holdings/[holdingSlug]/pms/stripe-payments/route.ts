import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {createPmsStripePayment} from "@erpx/server-pms/public/pms-stripe";
import {nativePaymentStatus} from "@erpx/server-pms/adapters/native-payment-status";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {holdingResources} from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import {isTransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";

type Context={params:Promise<{holdingSlug:string}>};
export async function GET(request:Request,{params}:Context){try{
 const actor=await requireWorkContext((await params).holdingSlug),id=new URL(request.url).searchParams.get("id");
 if(!id)throw new ApiError(422,"PMS_PAYMENT_STATUS_REQUEST_INVALID","Use one bounded payment status read.");
 return Response.json(canonicalDestinationLinks({data:await nativePaymentStatus(request.headers,actor,id)}),{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
export async function POST(request:Request,{params}:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the ERPX website');
 const actor=await requireWorkContext((await params).holdingSlug),key=request.headers.get('Idempotency-Key');
 if(!key)throw new ApiError(422,'IDEMPOTENCY_KEY_REQUIRED','Provide a payment request key');
 const {write}=await holdingResources(actor.holdingId);if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
 return Response.json(canonicalDestinationLinks({data:await createPmsStripePayment(write,actor,await request.json(),key)}),{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
