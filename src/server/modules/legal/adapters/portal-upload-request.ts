import 'server-only';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {sessionFromRequest} from '../../identity_access/public/auth';
import {withCurrentPrivateSessionRead} from '../../identity_access/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../runtime/public/runtime';
import {PostgresDatabase} from '../../runtime/public/runtime/postgres';
import {ApiError} from '../../runtime/public/http';
import {legalPortalActor} from '../domain/portal';
import {portalUpload,portalUploadRecovery} from './legal/portal';

/** The route buffers its bounded body before this uncached normal sign-in.
 * The original File staging is durable; only its original Legal binding and
 * receipt are conditional on final current request authority. */
export async function legalPortalUploadRequest(request:Request,holdingSlug:string,shareId:string,name:string,mediaType:string,bytes:Uint8Array,key:string){
 const originalBytes=new Uint8Array(bytes);
 const session=await sessionFromRequest(request);
 if(!session)throw new ApiError(401,'UNAUTHENTICATED','Sign in to upload the shared Legal document.');
 const actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message)),image=()=>JSON.stringify({session,headers:[...request.headers.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:actor.userId,email:actor.email,sessionId:actor.sessionId,audience:productHostForHost(request.headers.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The shared Legal upload sign-in changed.');await assertSession();};
  const binding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(binding.id);
  if(!(resources.write instanceof PostgresDatabase))throw new ApiError(503,'LEGAL_PORTAL_TRANSACTION_REQUIRED','The shared Legal upload requires its original Holding owner.');
  try{return await portalUpload(resources.write,resources.files,actor,shareId,name,mediaType,originalBytes,key,assertCurrent);}catch(error){
   if(request.headers.get('X-Legal-Upload-Recovery')!=='status-v1'||!(error instanceof ApiError)||error.code!=='LEGAL_VERSION_CONFLICT')throw error;
   const recovered=await portalUploadRecovery(resources.write,actor,shareId,name,mediaType,originalBytes,key,assertCurrent);
   if(recovered)return recovered;
   throw error;
  }
 });
}
