import { ApiError, errorResponse } from '@/server/http';
import { posStripeWebhook } from '@/server/pos-stripe';
export async function POST(request:Request){try{const raw=await request.text();if(raw.length>1000000)throw new ApiError(413,'PAYLOAD_TOO_LARGE','Payload too large.');return Response.json(await posStripeWebhook(raw,request.headers.get('stripe-signature')??''));}catch(error){return errorResponse(error);}}
