import {requireWorkContext} from '@/server/authorization';
import {requestPublicOrigin} from '@/server/public-origin';
import {ApiError,errorResponse} from '@/server/http';
import {crmWorkspace,crmDetail,createCRMLead,changeCRMLead,changeCRMOpportunity} from '@/server/crm';
import {changeCRMIdentity} from '@/server/crm-identities';
type Context={params:Promise<{holdingSlug:string}>};const headers={'Cache-Control':'private, no-store'};
export async function GET(r:Request,c:Context){try{const a=await requireWorkContext((await c.params).holdingSlug),id=new URL(r.url).searchParams.get('id');return Response.json({data:id?await crmDetail(a,id):await crmWorkspace(a)},{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();if(!input||typeof input!=='object')throw new ApiError(422,'COMMAND_REQUIRED','Provide a CRM command.');const data=input.domain==='identity'?await changeCRMIdentity(a,input,key):typeof input.id!=='string'?await createCRMLead(a,input,key):input.domain==='opportunity'?await changeCRMOpportunity(a,input.id,input,key):await changeCRMLead(a,input.id,input,key);return Response.json({data},{headers});}catch(e){return errorResponse(e);}}
