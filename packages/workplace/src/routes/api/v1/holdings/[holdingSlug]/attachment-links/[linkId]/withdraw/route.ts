import {boundedJsonBody} from '../../../../../../../../lib/bounded-json-body';
import {nativeFieldFamilyCommand,nativeFinancePaymentFiles,nativeFinanceJournalFiles} from '../../../../../../../../lib/native-field-family';
import {fieldFileWriterOwner,servicePaymentFileOwner,fieldJournalFileOwner} from '@erpx/server-files/public/field-file-writer-owner';
import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { requireWorkContext, type WorkContext } from "@erpx/server-identity_access/public/authorization";
import {
  createNativeAttachmentCommandClient,
  nativeAttachmentCommandConfiguration,
  nativeAttachmentCommandFailure,
  type NativeAttachmentPageContext,
} from "@erpx/server-files/public/native-attachments";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
const schema=z.object({reason:z.string().trim().min(3).max(1000)});

/** Rendering input only; the native route resolves the current authority. */
function nativePageContext(actor:WorkContext):NativeAttachmentPageContext{
  if(!actor.authority||!actor.iam)throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your work role changed. Reload before withdrawing files.");
  const scope=actor.scope.kind==='holding'?{kind:'holding' as const}:actor.scope.kind==='branch'?{kind:'branch' as const,branchSlug:actor.scope.branchSlug}:{kind:'department' as const,branchSlug:actor.scope.branchSlug,departmentSlug:actor.scope.departmentSlug};
  return {userId:actor.userId,holdingId:String(actor.holdingId),holdingSlug:actor.holdingSlug,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,assignmentId:actor.assignmentId,role:actor.role,assignmentScope:actor.assignmentScope,scope,authority:{revision:actor.authority.revision,assignmentVersion:actor.authority.assignmentVersion},modules:actor.modules,iam:{permissions:actor.iam.permissions}};
}

async function nativeGenericWithdraw(request:Request,actor:WorkContext,holdingSlug:string,linkId:string,reason:string,key:string){
  const raw=process.env.ERPX_NATIVE_ATTACHMENT_COMMAND_CONFIG;
  if(raw===undefined)throw new ApiError(503,"NATIVE_ATTACHMENT_WRITER_UNAVAILABLE","The native file writer is unavailable.");
  try{
    const client=createNativeAttachmentCommandClient(nativeAttachmentCommandConfiguration(raw));
    return await client.withdraw(request.headers,nativePageContext(actor),holdingSlug,linkId,{reason,key});
  }catch(error){
    const mapped=nativeAttachmentCommandFailure(error);
    if(mapped)throw new ApiError(mapped.status,mapped.code,mapped.message);
    throw error;
  }
}

export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/withdraw">){
  try{
    const origin=request.headers.get("origin");
    if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const key=request.headers.get("Idempotency-Key");
    if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");
    const {holdingSlug,linkId}=await context.params,actor=await requireWorkContext(holdingSlug),body=await boundedJsonBody(request,2*1024*1024),parsed=schema.safeParse(body.value);
    if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"A withdrawal reason is required.");
    if(await fieldJournalFileOwner(actor,{kind:"link",id:linkId}))return await nativeFinanceJournalFiles(request,actor,{route:{kind:"withdraw",linkId},body:body.raw,contentType:"application/json",key});
    if(await servicePaymentFileOwner(actor,{kind:"link",id:linkId}))return await nativeFinancePaymentFiles(request,actor,{route:{kind:"withdraw",linkId},body:body.raw,contentType:"application/json",key});
    if(await fieldFileWriterOwner(actor,{kind:"link",id:linkId})==="field")return await nativeFieldFamilyCommand(request,actor,"withdraw",linkId,{key,body:new TextDecoder().decode(body.raw)});
    const data=await nativeGenericWithdraw(request,actor,holdingSlug,linkId,parsed.data.reason,key);
    return Response.json(canonicalDestinationLinks({data}),{headers:{"Cache-Control":"private, no-store"}});
  }catch(error){return errorResponse(error);}
}
