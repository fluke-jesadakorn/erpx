import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {posStripeWebhook} from '@erpx/server-pos/public/pos-stripe';
import {holdingRoutingEnabled,holdingHostForHost} from '@erpx/feature-runtime/lib/holding-hosts';
export async function POST(request:Request){try{const raw=await request.text();if(raw.length>1000000)throw new ApiError(413,'PAYLOAD_TOO_LARGE','Payload too large.');const holding=holdingRoutingEnabled(process.env)?holdingHostForHost(request.headers.get('host')):undefined;return Response.json(canonicalDestinationLinks(await posStripeWebhook(raw,request.headers.get('stripe-signature')??'',holding?.holdingId)));}catch(error){return errorResponse(error);}}
