import {acknowledgeSupportAlert} from '@/server/support-sla-alerts';
import {requireWorkContext} from '@/server/authorization';
import {requestPublicOrigin} from '@/server/public-origin';
import {ApiError,errorResponse} from '@/server/http';
import {supportSlaWorkspace,createSupportPolicy,changeSupportPolicy,attachSupportClock} from '@/server/support-sla';
type Context={params:Promise<{holdingSlug:string}>};
export async function GET(r:Request,c:Context){try{return Response.json({data:await supportSlaWorkspace(await requireWorkContext((await c.params).holdingSlug))},{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();if(!input||typeof input!=='object')throw new ApiError(422,'COMMAND_REQUIRED','Provide an SLA command.');const data=input.action==='acknowledge'?await acknowledgeSupportAlert(a,input.id,input,key):input.action==='create-policy'?await createSupportPolicy(a,input,key):input.action==='attach'?await attachSupportClock(a,input.id,input,key):await changeSupportPolicy(a,input.id,input,key);return Response.json({data},{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
