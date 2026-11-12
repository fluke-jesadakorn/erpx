import {ssoAudienceForHost} from '@erpx/feature-runtime/lib/product-hosts';
import 'server-only';
import {sessionFromRequest} from '@erpx/server-identity_access/public/account-session';
import {withCurrentPrivateSessionRead} from '@erpx/server-identity_access/public/private-session-read';
import {completeAttachmentUploadForRequest,portalAttachmentCommandForRequest,portalAttachmentMetadataReadForRequest,portalAttachmentContentForRequest} from '@erpx/server-files/public/attachments/service';
import {createNativePortalAttachmentCommandClient,nativeAttachmentCommandConfiguration,nativeAttachmentCommandFailure} from '@erpx/server-files/public/native-attachments';
import {portalAttachmentWithdrawProof} from '@erpx/server-customer_authority/public/attachment-withdraw-proof';
import {appAttachmentSources} from '@erpx/server-files/public/app-attachment-sources';
import {portalAttachmentRequestSourceLocks,portalAttachmentSourceWitness} from '@erpx/server-customer_authority/public/attachment-request-sources';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {holdingBindingBySlug,holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase, type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {createCustomerPortalSessionApplication} from '@erpx/server-customer_360/application/portal-session';
import {customerPortalWorkContext} from '@erpx/server-customer_360/domain/portal';
import {customerPortalBindingScope,customerPortalAttachmentTarget} from '@erpx/server-customer_360/adapters/portal-attachments';

/** Re-read normal authentication after any bounded network body. The session
 * lease covers only original native effects, never an untrusted body stream. */
async function currentRequest(request:Request,holdingSlug:string){
 const session=await sessionFromRequest(request);
 if(!session)throw new ApiError(401,'UNAUTHENTICATED','Sign in to open customer files.');
 const application=createCustomerPortalSessionApplication({error:(status,code,message)=>new ApiError(status,code,message)}),user=application.verify(session.user);
 const image=()=>JSON.stringify({session,headers:[...request.headers.entries()]}),captured=image();
 return {session,application,user,assertImage:()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The verified customer sign-in changed.');},holdingSlug};
}

export async function customerPortalUploadRequest(request:Request,holdingSlug:string,uploadId:string,buffer:ArrayBuffer){
 const current=await currentRequest(request,holdingSlug);
 return withCurrentPrivateSessionRead({userId:current.user.userId,email:current.user.email,sessionId:current.session.session.id,audience:ssoAudienceForHost(request.headers.get('host'))},async assertSession=>{
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id),actor=current.application.actor(current.user,holding),bindingId=await customerPortalAttachmentTarget(resources.write,'upload',uploadId),binding=await customerPortalBindingScope(resources.write,actor,bindingId),attachmentActor=customerPortalWorkContext(actor,binding);
  const intent=await appAttachmentSources(resources.write).portalReservation(uploadId,actor.userId,binding.company_id);
  if(!intent)throw new ApiError(404,'UPLOAD_NOT_FOUND','Upload intent not found.');
  if(request.headers.get('content-type')?.split(';',1)[0]?.trim().toLowerCase()!==intent.media_type)throw new ApiError(422,'UPLOAD_TYPE','Content-Type must match the reserved file type.');
  if(buffer.byteLength!==Number(intent.declared_bytes))throw new ApiError(422,'UPLOAD_SIZE','The uploaded bytes must exactly match the reserved size.');
  let source:string|null|undefined;
  const assertCurrent=async(database:SqlDatabase)=>{
   current.assertImage();await assertSession();await lockScheduledFeatureSources(database);
   const actual=await customerPortalBindingScope(database,actor,bindingId),target=await customerPortalAttachmentTarget(database,'upload',uploadId),witness=await portalAttachmentSourceWitness(database,attachmentActor,bindingId);
   if(target!==bindingId||JSON.stringify(actual)!==JSON.stringify(binding)||!witness||source!==undefined&&source!==witness)throw new ApiError(409,'CUSTOMER_PORTAL_ATTACHMENT_CHANGED','The original customer document or file authority changed.');
   source=witness;
  };
  return completeAttachmentUploadForRequest(resources.write,resources.files,attachmentActor,uploadId,buffer,{sourceLocks:portalAttachmentRequestSourceLocks,assertCurrent});
 });
}

export async function customerPortalEvidenceRequest(request:Request,holdingSlug:string,linkId:string){
 const current=await currentRequest(request,holdingSlug);
 return withCurrentPrivateSessionRead({userId:current.user.userId,email:current.user.email,sessionId:current.session.session.id,audience:ssoAudienceForHost(request.headers.get('host'))},async assertSession=>{
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id),actor=current.application.actor(current.user,holding),bindingId=await customerPortalAttachmentTarget(resources.write,'link',linkId),binding=await customerPortalBindingScope(resources.write,actor,bindingId),attachmentActor=customerPortalWorkContext(actor,binding);
  const assertCurrent=async()=>{current.assertImage();await assertSession();};
  return portalAttachmentContentForRequest(resources.write,resources.files,attachmentActor,linkId,assertCurrent);
 });
}

export type CustomerPortalAttachmentCommand =
  | Parameters<typeof portalAttachmentCommandForRequest>[2]
  | {kind:'withdraw';linkId:string;reason:string;key:string};

export async function customerPortalAttachmentCommandRequest(request:Request,holdingSlug:string,command:CustomerPortalAttachmentCommand){
 const current=await currentRequest(request,holdingSlug);
 return withCurrentPrivateSessionRead({userId:current.user.userId,email:current.user.email,sessionId:current.session.session.id,audience:ssoAudienceForHost(request.headers.get('host'))},async assertSession=>{
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id),actor=current.application.actor(current.user,holding);
  const attachmentTarget=(database:SqlDatabase)=>command.kind==='create'?Promise.resolve(command.input.targetId):customerPortalAttachmentTarget(database,command.kind==='reserve'?'set':'link',command.kind==='reserve'?command.setId:command.linkId);
  const bindingId=await attachmentTarget(resources.write),binding=await customerPortalBindingScope(resources.write,actor,bindingId),attachmentActor=customerPortalWorkContext(actor,binding);
  if(command.kind==='withdraw'){
   const raw=process.env.ERPX_NATIVE_ATTACHMENT_COMMAND_CONFIG;
   if(raw===undefined)throw new ApiError(503,'NATIVE_ATTACHMENT_WRITER_UNAVAILABLE','The native file writer is unavailable.');
   if(!isTransactionalSqlDatabase(resources.write))throw new ApiError(503,'CUSTOMER_PORTAL_ATTACHMENT_TRANSACTION_REQUIRED','A customer file command requires its fresh native transaction.');
   await resources.write.transaction(database=>portalAttachmentWithdrawProof(database,attachmentActor,command.linkId));
   current.assertImage();await assertSession();
   try{return await createNativePortalAttachmentCommandClient(nativeAttachmentCommandConfiguration(raw)).withdraw(request.headers,holdingSlug,command.linkId,{reason:command.reason,key:command.key});}
   catch(error){const mapped=nativeAttachmentCommandFailure(error);if(mapped)throw new ApiError(mapped.status,mapped.code,mapped.message);throw error;}
  }
  let source:string|null|undefined;
  const assertCurrent=async(database:SqlDatabase)=>{
   current.assertImage();await assertSession();await lockScheduledFeatureSources(database);
   const actual=await customerPortalBindingScope(database,actor,bindingId),target=await attachmentTarget(database),witness=await portalAttachmentSourceWitness(database,attachmentActor,bindingId);
   if(target!==bindingId||JSON.stringify(actual)!==JSON.stringify(binding)||!witness||source!==undefined&&source!==witness)throw new ApiError(409,'CUSTOMER_PORTAL_ATTACHMENT_CHANGED','The original customer document or file authority changed.');
   source=witness;
  };
  return portalAttachmentCommandForRequest(resources.write,attachmentActor,command,{sourceLocks:portalAttachmentRequestSourceLocks,assertCurrent});
 });
}

export async function customerPortalAttachmentMetadataRequest(request:Request,holdingSlug:string,read:Parameters<typeof portalAttachmentMetadataReadForRequest>[2]){
 const current=await currentRequest(request,holdingSlug);
 return withCurrentPrivateSessionRead({userId:current.user.userId,email:current.user.email,sessionId:current.session.session.id,audience:ssoAudienceForHost(request.headers.get('host'))},async assertSession=>{
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id),actor=current.application.actor(current.user,holding),bindingId=read.kind==='sets'?read.input.targetId:await customerPortalAttachmentTarget(resources.write,'link',read.linkId),binding=await customerPortalBindingScope(resources.write,actor,bindingId),attachmentActor=customerPortalWorkContext(actor,binding);
  let source:string|null|undefined;
  const assertCurrent=async(database:SqlDatabase)=>{
   current.assertImage();await assertSession();await lockScheduledFeatureSources(database);
   const actual=await customerPortalBindingScope(database,actor,bindingId),target=read.kind==='sets'?read.input.targetId:await customerPortalAttachmentTarget(database,'link',read.linkId),witness=await portalAttachmentSourceWitness(database,attachmentActor,bindingId);
   if(target!==bindingId||JSON.stringify(actual)!==JSON.stringify(binding)||!witness||source!==undefined&&source!==witness)throw new ApiError(409,'CUSTOMER_PORTAL_ATTACHMENT_CHANGED','The original customer document or file authority changed.');
   source=witness;
  };
  return portalAttachmentMetadataReadForRequest(resources.write,attachmentActor,read,{sourceLocks:portalAttachmentRequestSourceLocks,assertCurrent});
 });
}
