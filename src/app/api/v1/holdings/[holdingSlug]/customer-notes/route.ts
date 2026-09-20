import {requireWorkContext} from '@/server/authorization';
import {requestPublicOrigin} from '@/server/public-origin';
import {ApiError,errorResponse} from '@/server/http';
import {createCustomerNote,changeCustomerNote,customerNotesForOrder} from '@/server/customer-notes';
type Context={params:Promise<{holdingSlug:string}>};
const headers={'Cache-Control':'private, no-store'};
export async function GET(r:Request,c:Context){try{const orderId=new URL(r.url).searchParams.get('orderId');if(!orderId)throw new ApiError(422,'ORDER_REQUIRED','Choose the source order.');return Response.json({data:await customerNotesForOrder(await requireWorkContext((await c.params).holdingSlug),orderId)},{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();return Response.json({data:typeof input.id==='string'?await changeCustomerNote(a,input.id,input,key):await createCustomerNote(a,input,key)},{headers});}catch(e){return errorResponse(e);}}
