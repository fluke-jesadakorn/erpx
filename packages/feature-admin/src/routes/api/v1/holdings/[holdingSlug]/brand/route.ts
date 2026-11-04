import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {brandIdentityWorkspace,saveBrandIdentityDraft,publishBrandIdentity,rollbackBrandIdentity,changeBrandCategoryLock,cancelBrandDraft} from "@erpx/server-brand/public/brand-identity";
import {nativeBrandIdentityCommand,nativeBrandIdentityFailure,nativeBrandIdentityWorkspace} from "@erpx/feature-brand/lib/native-brand-identity";
type Context={params:Promise<{holdingSlug:string}>};const headers={"Cache-Control":"private, no-store"};
function nativeFailure(error:unknown):never{const failure=nativeBrandIdentityFailure(error);if(failure)throw new ApiError(failure.status,failure.code,failure.message);throw error;}
export async function GET(_:Request,c:Context){try{const actor=await requireWorkContext((await c.params).holdingSlug);let data;try{data=(await nativeBrandIdentityWorkspace(actor,actor.holdingSlug))??await brandIdentityWorkspace(actor);}catch(error){nativeFailure(error);}return Response.json(canonicalDestinationLinks({data}),{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{
 if(r.headers.get("origin")&&r.headers.get("origin")!==requestPublicOrigin(r))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const key=r.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");
 const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();let data;
 try{data=await nativeBrandIdentityCommand(a,a.holdingSlug,input,key);}catch(error){nativeFailure(error);}
 if(data===undefined){
  if(input?.action==="save")data=await saveBrandIdentityDraft(a,input,key);
  else if(input?.action==="publish"&&typeof input.id==="string")data=await publishBrandIdentity(a,input.id,input,key);
  else if(input?.action==="cancel"&&typeof input.id==="string")data=await cancelBrandDraft(a,input.id,input,key);
  else if(input?.action==="rollback")data=await rollbackBrandIdentity(a,input,key);
  else if(input?.action==="category-lock")data=await changeBrandCategoryLock(a,input,key);
  else throw new ApiError(422,"BRAND_COMMAND_REQUIRED","Choose a Brand identity command.");
 }
 return Response.json(canonicalDestinationLinks({data}),{headers});
}catch(e){return errorResponse(e);}}
