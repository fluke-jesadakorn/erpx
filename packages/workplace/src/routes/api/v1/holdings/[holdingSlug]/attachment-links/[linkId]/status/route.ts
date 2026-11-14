import {nativeFieldFamily,nativeFinancePaymentFiles,nativeFinanceJournalFiles} from '../../../../../../../../lib/native-field-family';
import {nativeAttachmentStatus} from '../../../../../../../../lib/native-attachments';
import {fieldFileWriterOwner,servicePaymentFileOwner,fieldJournalFileOwner} from '@erpx/server-files/public/field-file-writer-owner';
import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { attachmentLinkStatus } from "@erpx/server-files/public/attachments/service";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
import {nativeProjectAttachmentStatusByLink} from "../../../../../../../../../../../packages/feature-projects/src/public/native-project-attachment-read";
import {nativeProjectsAvailable,nativeProjectsFailure,nativeProjectsPageContext} from "../../../../../../../../../../../packages/feature-projects/src/public/native-project-wbs-page";
import {NativeProjectsError} from "../../../../../../../../../../../packages/feature-projects/src/public/native-project-wbs";

function hiddenProjectAttachment(error:unknown){
  return error instanceof NativeProjectsError&&(error.code==="ATTACHMENT_LINK_NOT_FOUND"||error.code==="ATTACHMENT_SET_NOT_FOUND");
}

export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/status">){try{const {holdingSlug,linkId}=await context.params,actor=await requireWorkContext(holdingSlug);if(await fieldJournalFileOwner(actor,{kind:"link",id:linkId}))return await nativeFinanceJournalFiles(request,actor,{route:{kind:"status",linkId}});if(await servicePaymentFileOwner(actor,{kind:"link",id:linkId}))return await nativeFinancePaymentFiles(request,actor,{route:{kind:"status",linkId}});if(await fieldFileWriterOwner(actor,{kind:"link",id:linkId})==="field")return await nativeFieldFamily(request,actor,{route:{kind:"status",linkId}});if(nativeProjectsAvailable()){try{const data=await nativeProjectAttachmentStatusByLink(request.headers,nativeProjectsPageContext(actor),linkId);return Response.json(canonicalDestinationLinks({data}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){if(!hiddenProjectAttachment(error)){const failure=nativeProjectsFailure(error);if(failure)throw new ApiError(failure.status,failure.code,failure.message);throw error;}}}const {write}=await holdingResources(actor.holdingId);const native=await nativeAttachmentStatus(request,actor,linkId);if(native)return native;return Response.json(canonicalDestinationLinks({data:await attachmentLinkStatus(write,actor,linkId)}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
