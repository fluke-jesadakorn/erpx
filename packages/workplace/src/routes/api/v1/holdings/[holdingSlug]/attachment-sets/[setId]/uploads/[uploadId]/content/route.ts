import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {nativeProjectAttachmentCompleteUpload,nativeProjectAttachmentConfigured} from "@erpx/feature-projects/public/native-project-attachment";
import {nativeProjectsFailure,nativeProjectsPageContext} from "@erpx/feature-projects/public/native-project-wbs-page";
import {projectAttachmentTarget} from "@erpx/server-files/public/project-attachment";
import {readBoundedUploadBody} from "@erpx/server-files/public/attachments/upload";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { holdingResources } from "../../../../../../../../../../../../../packages/server-runtime/src/public/runtime";

export async function PUT(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/uploads/[uploadId]/content">,
) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const { holdingSlug, setId, uploadId } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    if (!nativeProjectAttachmentConfigured() || await projectAttachmentTarget(write, actor.companyId, setId) !== "project")
      throw new ApiError(404, "ATTACHMENT_UPLOAD_NOT_FOUND", "Upload intent not found.");
    const contentType = request.headers.get("content-type")?.split(";", 1)[0]?.trim().toLowerCase() ?? "";
    const buffer = await readBoundedUploadBody(request, Number(request.headers.get("content-length")));
    try {
      const data = await nativeProjectAttachmentCompleteUpload(
        request.headers,
        nativeProjectsPageContext(actor),
        setId,
        uploadId,
        contentType,
        new Uint8Array(buffer),
      );
      return Response.json(canonicalDestinationLinks({data}), { status: 202 });
    } catch (error) {
      const failure = nativeProjectsFailure(error);
      if (failure) throw new ApiError(failure.status, failure.code, failure.message);
      throw error;
    }
  } catch (error) {
    return errorResponse(error);
  }
}
