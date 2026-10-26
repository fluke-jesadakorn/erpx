import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {operationsWorkspace,operationsExceptionDetail,draftQueuePolicy,reviewQueuePolicy,openOperationsException,changeOperationsException} from "@erpx/server-operations/public/operations-exceptions";
type Context={params:Promise<{holdingSlug:string}>};
const headers={"Cache-Control":"private, no-store"};
export async function GET(r:Request,c:Context){
 try{const a=await requireWorkContext((await c.params).holdingSlug),id=new URL(r.url).searchParams.get("id");return Response.json(canonicalDestinationLinks({data:id?await operationsExceptionDetail(a,id):await operationsWorkspace(a)}),{headers});}catch(e){return errorResponse(e);}
}
export async function POST(r:Request,c:Context){
 try{
  if(r.headers.get("origin")&&r.headers.get("origin")!==requestPublicOrigin(r))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
  const key=r.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");
  const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();
  if(!input||typeof input!=="object"||!["policy","exception"].includes(input.entity))throw new ApiError(422,"OPERATIONS_ENTITY_REQUIRED","Choose a policy or exception command.");
  const data=input.entity==="policy"?typeof input.id==="string"?await reviewQueuePolicy(a,input.id,input,key):await draftQueuePolicy(a,input,key):typeof input.id==="string"?await changeOperationsException(a,input.id,input,key):await openOperationsException(a,input,key);
  return Response.json(canonicalDestinationLinks({data}),{headers});
 }catch(e){return errorResponse(e);}
}
