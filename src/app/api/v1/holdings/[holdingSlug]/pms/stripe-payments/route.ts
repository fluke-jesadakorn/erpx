import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {createPmsStripePayment,pmsStripePaymentStatus} from "@/server/modules/pms/public/pms-stripe";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {isTransactionalSqlDatabase} from "@/server/modules/runtime/public/runtime/types";

type Context={params:Promise<{holdingSlug:string}>};
export async function POST(request:Request,{params}:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the ERPX website');
 const actor=await requireWorkContext((await params).holdingSlug),key=request.headers.get('Idempotency-Key');
 if(!key)throw new ApiError(422,'IDEMPOTENCY_KEY_REQUIRED','Provide a payment request key');
 const {write}=await holdingResources(actor.holdingId);if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
 return Response.json({data:await createPmsStripePayment(write,actor,await request.json(),key)},{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
export async function GET(request:Request,{params}:Context){try{
 const actor=await requireWorkContext((await params).holdingSlug),id=new URL(request.url).searchParams.get('id')??'';
 const {write}=await holdingResources(actor.holdingId);if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
 return Response.json({data:await pmsStripePaymentStatus(write,actor,id)},{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
