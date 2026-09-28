import { requestPublicOrigin } from "@/server/public-origin";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { ApiError, errorResponse } from "@/server/http";
import {
  bindCompatibilityAttachmentSet,
  completeAttachmentUpload,
  scanAttachmentNow,
} from "@/server/attachments/service";
import { attachmentsV2ReadOnly } from "@/server/attachments/feature";
import { readBoundedUploadBody } from "@/server/attachments/upload";
import { legacyAttachmentPolicy } from "@/server/attachments/legacy-policy";

export async function PUT(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/uploads/[id]/content">,
) {
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
    const { write, files } = await holdingResources(actor.holdingId);
    const universal = await write
      .prepare(
        `SELECT u.declared_bytes,u.media_type,u.attachment_set_id,u.owner_type,u.purpose,
           s.component_kind,s.component_ref,s.purpose_key,s.expected_target_version,
           EXISTS(SELECT 1 FROM attachment_events event WHERE event.set_id=s.id AND event.company_id=s.company_id
             AND event.action='set.created' AND COALESCE((event.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=true) compatibility_adapter
         FROM upload_intents u
         JOIN attachment_sets s ON s.id=u.attachment_set_id
         WHERE u.id=? AND u.created_by=? AND u.company_id=?`,
      )
      .bind(id, actor.userId, actor.companyId)
      .first<{
        declared_bytes: number;
        media_type: string;
        attachment_set_id: string;
        owner_type: string;
        purpose: string;
        component_kind: "record" | "action" | "line" | "event" | "message" | "publication";
        component_ref: string | null;
        purpose_key: string;
        expected_target_version: number;
        compatibility_adapter: boolean;
      }>();
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
    const uploaded = await completeAttachmentUpload(
      write,
      files,
      actor,
      id,
      buffer,
      { compatibilityAdapter },
    );
    if (compatibilityAdapter) {
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
    return errorResponse(error);
  }
}
