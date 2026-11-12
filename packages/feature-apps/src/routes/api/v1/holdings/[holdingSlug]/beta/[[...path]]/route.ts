import {workSocialBetaPathAllowed} from '@erpx/contracts/work-social-api';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {betaHandler} from '@erpx/server-manage_erpx/public/beta-api';
import {
  nativeCommentUploadCancel,
  nativeCommentUploadCreate,
  nativeEditUploadCancel,
  nativeEditUploadCreate,
  nativePublishDraft,
  nativeSaveDraft,
  nativeSocialAvailable,
} from '@erpx/server-collaboration/public/native-social-page';

const native=betaHandler('beta');
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const headers={'Cache-Control':'private, no-store'};

async function nativeSocialBeta(request:Request,holdingSlug:string,path:string[]){
  const key=request.headers.get('Idempotency-Key')??'';
  if(!key)return undefined;
  const body=request.method==='POST'?await request.clone().json().catch(()=>null) as Record<string,unknown>|null:{};
  if(body===null)return undefined;
  if(path[0]==='drafts'){
    if(path.length===1)return Response.json({data:await nativeSaveDraft(request,holdingSlug,body,key)},{headers});
    if(path.length===2)return Response.json({data:await nativeSaveDraft(request,holdingSlug,{...body,id:path[1]},key)},{headers});
    if(path.length===3&&path[2]==='publish')return Response.json({data:await nativePublishDraft(request,holdingSlug,path[1]!,Number(body.expectedVersion),key)},{headers});
  }
  if(path[0]==='comment-uploads'){
    if(path.length===1)return Response.json({data:await nativeCommentUploadCreate(request,holdingSlug,String(body.postId??''),key)},{headers});
    if(path.length===2)return Response.json({data:await nativeCommentUploadCancel(request,holdingSlug,path[1]!,key)},{headers});
  }
  if(path[0]==='post-edit-uploads'){
    if(path.length===1)return Response.json({data:await nativeEditUploadCreate(request,holdingSlug,String(body.postId??''),Number(body.expectedVersion),key)},{headers});
    if(path.length===2)return Response.json({data:await nativeEditUploadCancel(request,holdingSlug,path[1]!,key)},{headers});
  }
  return undefined;
}

export async function workSocialRequest(request:Request,context:Context){
 const{holdingSlug,path=[]}=await context.params;
 if(productHostForHost(request.headers.get('host'))!=='apps'||!workSocialBetaPathAllowed(path,request.method))return Response.json({error:{code:'NOT_FOUND',message:'Resource unavailable.'}},{status:404,headers});
 if(nativeSocialAvailable()){
  const handled=await nativeSocialBeta(request,holdingSlug,path);
  if(handled)return handled;
 }
 // HEAD has GET semantics; the framework removes its response body. The native
 // command dispatcher must never interpret it as a mutation.
 return native(request.method==='HEAD'?new Request(request,{method:'GET'}):request,context);
}
export const GET=workSocialRequest;
export const POST=workSocialRequest;
