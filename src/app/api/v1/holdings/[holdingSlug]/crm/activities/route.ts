import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {crmActivityWorkspace,createCRMActivity,changeCRMActivity} from "@/server/modules/crm/public/crm-activities";
type Context={params:Promise<{holdingSlug:string}>};
const headers={'Cache-Control':'private, no-store'};
export async function GET(r:Request,c:Context){try{const a=await requireWorkContext((await c.params).holdingSlug),leadId=new URL(r.url).searchParams.get('leadId');if(!leadId)throw new ApiError(422,'LEAD_REQUIRED','Choose a CRM lead.');return Response.json(canonicalDestinationLinks({data:await crmActivityWorkspace(a,leadId)}),{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();if(!input||typeof input!=='object')throw new ApiError(422,'COMMAND_REQUIRED','Provide a CRM activity command.');const data=typeof input.id==='string'?await changeCRMActivity(a,input.id,input,key):await createCRMActivity(a,input,key);return Response.json(canonicalDestinationLinks({data}),{headers});}catch(e){return errorResponse(e);}}
