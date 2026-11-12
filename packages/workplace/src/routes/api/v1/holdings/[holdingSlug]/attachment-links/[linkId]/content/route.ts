import {nativeFieldFamily,nativeFinancePaymentFiles,nativeFinanceJournalFiles} from '../../../../../../../../lib/native-field-family';
import {fieldFileWriterOwner,servicePaymentFileOwner,fieldJournalFileOwner} from '@erpx/server-files/public/field-file-writer-owner';
import type { RouteContext } from "@erpx/contracts/next-routes";
import { attachmentContent } from "@erpx/server-files/public/attachments/service";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
import {nativeProjectAttachmentContentByLink} from "../../../../../../../../../../../packages/feature-projects/src/public/native-project-attachment-read";
import {nativeProjectsAvailable,nativeProjectsFailure,nativeProjectsPageContext} from "../../../../../../../../../../../packages/feature-projects/src/public/native-project-wbs-page";
import {NativeProjectsError} from "../../../../../../../../../../../packages/feature-projects/src/public/native-project-wbs";

function hiddenProjectAttachment(error:unknown){
  return error instanceof NativeProjectsError&&(error.code==="ATTACHMENT_LINK_NOT_FOUND"||error.code==="ATTACHMENT_SET_NOT_FOUND");
}

export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/content">){try{const {holdingSlug,linkId}=await context.params,actor=await requireWorkContext(holdingSlug),variant=new URL(request.url).searchParams.get("variant")??"original";if(variant!=="original"&&variant!=="preview")throw new ApiError(422,"ATTACHMENT_VARIANT","Choose original or preview.");if(await fieldJournalFileOwner(actor,{kind:"link",id:linkId}))return await nativeFinanceJournalFiles(request,actor,{route:{kind:"content",linkId,variant}});if(await servicePaymentFileOwner(actor,{kind:"link",id:linkId}))return await nativeFinancePaymentFiles(request,actor,{route:{kind:"content",linkId,variant}});if(await fieldFileWriterOwner(actor,{kind:"link",id:linkId})==="field")return await nativeFieldFamily(request,actor,{route:{kind:"content",linkId,variant}});if(nativeProjectsAvailable()){try{return await nativeProjectAttachmentContentByLink(request.headers,nativeProjectsPageContext(actor),linkId,variant);}catch(error){if(!hiddenProjectAttachment(error)){const failure=nativeProjectsFailure(error);if(failure)throw new ApiError(failure.status,failure.code,failure.message);throw error;}}}const {write,files}=await holdingResources(actor.holdingId);const result=await attachmentContent(write,files,actor,linkId,variant),headers=new Headers({"Content-Type":result.mediaType,"Content-Disposition":`${result.inline?"inline":"attachment"}; filename*=UTF-8''${encodeURIComponent(result.inline?`preview-${result.filename}`:result.filename)}`,"Cache-Control":"private, no-store","X-Content-Type-Options":"nosniff","Accept-Ranges":"none"});if(result.inline)headers.set("Content-Security-Policy","sandbox; default-src 'none'");return new Response(result.object.body,{headers});}catch(error){return errorResponse(error);}}
