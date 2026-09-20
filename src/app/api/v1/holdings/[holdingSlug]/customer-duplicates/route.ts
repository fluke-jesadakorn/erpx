import {requireWorkContext} from '@/server/authorization';
import {requestPublicOrigin} from '@/server/public-origin';
import {ApiError,errorResponse} from '@/server/http';
import {changeCustomerDuplicate,customerDuplicateDetail,customerDuplicateWorkspace,requestCustomerDuplicate} from '@/server/customer-duplicates';
type Context={params:Promise<{holdingSlug:string}>};const headers={'Cache-Control':'private, no-store'};
export async function GET(r:Request,c:Context){try{const a=await requireWorkContext((await c.params).holdingSlug),id=new URL(r.url).searchParams.get('id');return Response.json({data:id?await customerDuplicateDetail(a,id):await customerDuplicateWorkspace(a)},{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();return Response.json({data:typeof input?.id==='string'?await changeCustomerDuplicate(a,input.id,input,key):await requestCustomerDuplicate(a,input,key)},{headers});}catch(e){return errorResponse(e);}}
