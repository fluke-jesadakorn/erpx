import {appAttachmentSources} from "@/server/modules/files/public/app-attachment-sources";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import {
  bindCompatibilityAttachmentSet,
  completeAttachmentUpload,
  scanAttachmentNow,
} from "@/server/modules/files/public/attachments/service";
import { attachmentsV2ReadOnly } from "@/server/modules/files/public/attachments/feature";
import { readBoundedUploadBody } from "@/server/modules/files/public/attachments/upload";
import { legacyAttachmentPolicy } from "@/server/modules/files/public/attachments/legacy-policy";

export async function PUT(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/uploads/[id]/content">,
) {
  const requestId = crypto.randomUUID();
  let operation = "authorize";
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const { holdingSlug, id } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    if (attachmentsV2ReadOnly())
      throw new ApiError(
        503,
        "ATTACHMENTS_READ_ONLY",
        "Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.",
      );
    operation = "open_resources";
    const { write, files } = await holdingResources(actor.holdingId);
    operation = "load_reservation";
    const universal = await appAttachmentSources(write).universalReservation(id,actor.userId,actor.companyId);
    if (!universal)
      throw new ApiError(
        404,
        "UPLOAD_NOT_FOUND",
        "Upload intent was not migrated to an authorized attachment set.",
      );
    const contentType = request.headers
      .get("content-type")
      ?.split(";", 1)[0]
      ?.trim()
      .toLowerCase();
    if (contentType !== universal.media_type)
      throw new ApiError(
        422,
        "UPLOAD_TYPE",
        "Content-Type must match the reserved file type.",
      );
    operation = "read_body";
    const buffer = await readBoundedUploadBody(
      request,
      Number(universal.declared_bytes),
    );
    const compatibilityAdapter = Boolean(universal.compatibility_adapter);
    const mapped = compatibilityAdapter
      ? legacyAttachmentPolicy(universal.owner_type, universal.purpose)
      : null;
    if (
      compatibilityAdapter &&
      (!mapped ||
        mapped.purposeKey !== universal.purpose_key ||
        mapped.componentKind !== universal.component_kind ||
        (mapped.componentRef === null
          ? universal.owner_type !== "staff_task" ||
            !universal.component_ref?.startsWith("task:")
          : mapped.componentRef !== universal.component_ref))
    )
      throw new ApiError(
        409,
        "ATTACHMENT_COMPATIBILITY_CONTEXT_CHANGED",
        "This legacy upload no longer matches an approved domain binding context.",
      );
    operation = "complete_upload";
    const uploaded = await completeAttachmentUpload(
      write,
      files,
      actor,
      id,
      buffer,
      { compatibilityAdapter, requestId },
    );
    if (compatibilityAdapter) {
      operation = "scan";
      const state = await scanAttachmentNow(write, files, uploaded.sourceId);
      if (state !== "available")
        throw new ApiError(
          423,
          "ATTACHMENT_NOT_AVAILABLE",
          "The file did not pass security scanning.",
        );
      if (mapped!.completion === "stage_for_domain")
        return Response.json(
          {
            data: {
              ...uploaded,
              id: uploaded.sourceId,
              state,
              attachmentSetId: universal.attachment_set_id,
            },
          },
          { status: 202 },
        );
      operation = "bind";
      const result = await bindCompatibilityAttachmentSet(
        write,
        actor,
        universal.attachment_set_id,
        Number(universal.expected_target_version),
        `legacy-upload-bind-${id}`,
        { compatibilityAdapter: true },
      );
      return Response.json({ data: result }, { status: 201 });
    }
    return Response.json({ data: uploaded }, { status: 202 });
  } catch (error) {
    const response = errorResponse(error instanceof Error && error.name === "XMinioStorageFull"
      ? new ApiError(507, "UPLOAD_STORAGE_FULL", "File storage is temporarily full. Your draft is saved; try the upload again later.")
      : error);
    if (response.status >= 500) {
      const errorCode = error && typeof error === "object" && "code" in error &&
        typeof error.code === "string" && /^[A-Za-z0-9_]{1,80}$/.test(error.code)
        ? error.code : undefined;
      const errorName = error instanceof Error && /^[A-Za-z0-9_]{1,80}$/.test(error.name)
        ? error.name : typeof error;
      console.error(JSON.stringify({
        event: "attachment_upload_failed",
        requestId,
        operation,
        errorName,
        errorCode,
      }));
      const body = await response.json() as { error: { code: string; message: string } };
      return Response.json({ error: { ...body.error, requestId } }, {
        status: response.status,
        headers: { "X-Request-ID": requestId },
      });
    }
    return response;
  }
}
