import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {approvePmsStripeRefund,requestPmsStripeRefund,recoverPmsStripeRefund} from "@erpx/server-pms/public/pms-stripe";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {holdingResources} from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import {isTransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";

type Context={params:Promise<{holdingSlug:string}>};
export async function POST(request:Request,{params}:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the ERPX website');
 const actor=await requireWorkContext((await params).holdingSlug),body=await request.json();
 if(typeof body?.id!=='string')throw new ApiError(422,'PMS_REFUND_ID_REQUIRED','Select a payment');
 const {write}=await holdingResources(actor.holdingId);if(!isTransactionalSqlDatabase(write))throw new Error('POSTGRES_REQUIRED');
 const data=body.action==='recover'?await recoverPmsStripeRefund(write,actor,body.id,typeof body.reason==='string'?body.reason:'',body.payerEmail):body.action==='approve'?await approvePmsStripeRefund(write,actor,body.id):body.action==='request'?await requestPmsStripeRefund(write,actor,body.id,body.amountMinor,body.reason):null;
 if(!data)throw new ApiError(422,'PMS_REFUND_ACTION','Select a refund action');
 return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
