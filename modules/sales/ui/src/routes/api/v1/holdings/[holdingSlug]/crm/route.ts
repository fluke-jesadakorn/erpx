import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {oasisDemoStagingEnabled} from "@erpx/platform-contracts/holding-hosts";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {requestPublicOrigin} from "@erpx/manage-erpx-server/public/public-origin";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {crmWorkspace,crmDetail,createCRMLead,changeCRMLead,changeCRMOpportunity} from "@erpx/crm-server/public/crm";
import {changeCRMIdentity} from "@erpx/crm-server/public/crm-identities";
import {nativeGuestHistoryGet} from "@erpx/crm-server/public/crm-identities";
import {nativeCrmCommandOrThrow,nativeCrmCommandRequired} from "@erpx/sales-ui/public/native-crm-command";
type Context={params:Promise<{holdingSlug:string}>};const headers={'Cache-Control':'private, no-store'};
export async function GET(r:Request,c:Context){try{if(oasisDemoStagingEnabled(process.env))throw new ApiError(404,'NOT_FOUND','Not found.');if(new URL(r.url).searchParams.get('view')==='guest-history')return nativeGuestHistoryGet(r,c);const a=await requireWorkContext((await c.params).holdingSlug),id=new URL(r.url).searchParams.get('id');return Response.json(canonicalDestinationLinks({data:id?await crmDetail(a,id):await crmWorkspace(a)}),{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(oasisDemoStagingEnabled(process.env))throw new ApiError(404,'NOT_FOUND','Not found.');if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const holdingSlug=(await c.params).holdingSlug,a=await requireWorkContext(holdingSlug),input=await r.json();if(!input||typeof input!=='object')throw new ApiError(422,'COMMAND_REQUIRED','Provide a CRM command.');
 // Native fence: lead reassign/disqualify/qualify/reopen; opportunity link-quote/negotiate/lose/reopen/win/win-pms. Create, identity and activities keep their retained owner paths. Qualify, link-quote and win-pms are native-only: an absent native transport fails closed (503) instead of returning to a deleted TypeScript writer.
 const native=await nativeCrmCommandOrThrow(r,a,holdingSlug,input,key);if(native)return Response.json(canonicalDestinationLinks({data:native}),{headers});
 if(nativeCrmCommandRequired(input))throw new ApiError(503,'NATIVE_CRM_COMMAND_UNAVAILABLE','The native CRM command service is unavailable.');
 const data=input.domain==='identity'?await changeCRMIdentity(a,input,key):typeof input.id!=='string'?await createCRMLead(a,input,key):input.domain==='opportunity'?await changeCRMOpportunity(a,input.id,input,key):await changeCRMLead(a,input.id,input,key);return Response.json(canonicalDestinationLinks({data}),{headers});}catch(e){return errorResponse(e);}}
