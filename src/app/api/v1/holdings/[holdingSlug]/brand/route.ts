import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {brandIdentityWorkspace,saveBrandIdentityDraft,publishBrandIdentity,rollbackBrandIdentity,changeBrandCategoryLock,cancelBrandDraft} from "@/server/modules/brand/public/brand-identity";
type Context={params:Promise<{holdingSlug:string}>};const headers={"Cache-Control":"private, no-store"};
export async function GET(_:Request,c:Context){try{return Response.json({data:await brandIdentityWorkspace(await requireWorkContext((await c.params).holdingSlug))},{headers});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{
 if(r.headers.get("origin")&&r.headers.get("origin")!==requestPublicOrigin(r))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const key=r.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");
 const a=await requireWorkContext((await c.params).holdingSlug),input=await r.json();let data;
 if(input?.action==="save")data=await saveBrandIdentityDraft(a,input,key);
 else if(input?.action==="publish"&&typeof input.id==="string")data=await publishBrandIdentity(a,input.id,input,key);
 else if(input?.action==="cancel"&&typeof input.id==="string")data=await cancelBrandDraft(a,input.id,input,key);
 else if(input?.action==="rollback")data=await rollbackBrandIdentity(a,input,key);
 else if(input?.action==="category-lock")data=await changeBrandCategoryLock(a,input,key);
 else throw new ApiError(422,"BRAND_COMMAND_REQUIRED","Choose a Brand identity command.");
 return Response.json({data},{headers});
}catch(e){return errorResponse(e);}}
