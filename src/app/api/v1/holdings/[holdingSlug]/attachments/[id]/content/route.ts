import {getSupportTicket} from "@/server/support";
import { invoicePaymentOwner } from "@/server/invoice-payment-evidence";
import { getEmployeeRequest } from "@/server/employee-requests";
import { socialUploadOwner } from "@/server/social-home";
import { getTask } from "@/server/staff-tasks";
import { getExpense } from "@/server/expenses";
import { getLeave } from "@/server/leave";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { ApiError, errorResponse } from "@/server/http";
import { paymentDraft } from "@/server/expense-payments";
import { getQualityHold } from "@/server/quality";
import { getOrder } from "@/server/commerce";

export async function GET(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/attachments/[id]/content">,
) {
  try {
    const { holdingSlug, id } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const { write, files } = await holdingResources(actor.holdingId);
    const attachment = await write
      .prepare(
        "SELECT object_key,media_type,original_name,owner_type,owner_id FROM attachments WHERE id=? AND revoked_at IS NULL",
      )
      .bind(id)
      .first<{
        object_key: string;
        media_type: string;
        original_name: string;
        owner_type: string;
        owner_id: string;
      }>();
    if (!attachment)
      throw new ApiError(404, "NOT_FOUND", "Attachment not found.");
    if (attachment.owner_type === "commerce_order_delivery") await getOrder(actor,attachment.owner_id);
    else if (attachment.owner_type === "quality_hold") await getQualityHold(actor,attachment.owner_id);
    else if (attachment.owner_type === "support_ticket") await getSupportTicket(actor,attachment.owner_id);
    else if (attachment.owner_type === "employee_request") await getEmployeeRequest(write,actor,attachment.owner_id);
    else if (attachment.owner_type === "social_draft")
      await socialUploadOwner(write, actor, attachment.owner_id);
    else if (attachment.owner_type === "expense_claim")
      await getExpense(actor, attachment.owner_id);
    else if (attachment.owner_type === "invoice_payment")
      await invoicePaymentOwner(write,actor,attachment.owner_id);
    else if (attachment.owner_type === "expense_payment")
      await paymentDraft(actor, attachment.owner_id);
    else if (attachment.owner_type === "leave_request")
      await getLeave(actor.holdingId, attachment.owner_id, actor);
    else if (attachment.owner_type === "staff_task")
      await getTask(write, actor, attachment.owner_id);
    else if (["holding_brand", "branch_brand"].includes(attachment.owner_type)) {
      const active = await write.prepare(
        "SELECT 1 FROM organization_brand_profiles WHERE scope_kind=? AND scope_id=? AND (primary_attachment_id=? OR inverse_attachment_id=?)",
      ).bind(attachment.owner_type === "holding_brand" ? "holding" : "branch", attachment.owner_id, id, id).first();
      if (!active) throw new ApiError(404, "NOT_FOUND", "Workspace logo not found.");
    }
    else throw new ApiError(404, "NOT_FOUND", "Attachment not found.");
    const page = new URL(request.url).searchParams.get("page");
    if (page) {
      const derived = await write
        .prepare(
          "SELECT object_key FROM attachment_derivatives WHERE attachment_id=? AND company_id=? AND page_number=? AND original_checksum=(SELECT checksum_sha256 FROM attachments WHERE id=?)",
        )
        .bind(id, actor.companyId, Number(page), id)
        .first<{ object_key: string }>();
      if (!derived)
        throw new ApiError(
          404,
          "PAGE_NOT_FOUND",
          "This rendered page is unavailable.",
        );
      attachment.object_key = derived.object_key;
      attachment.media_type = "image/jpeg";
      attachment.original_name = `${attachment.original_name} · page ${page}.jpg`;
    }
    const rangeHeader = request.headers.get("range");
    const rangeMatch = rangeHeader?.match(/^bytes=(\d+)-(\d*)$/);
    const range = rangeMatch
      ? {
          offset: Number(rangeMatch[1]),
          length: rangeMatch[2]
            ? Number(rangeMatch[2]) - Number(rangeMatch[1]) + 1
            : undefined,
        }
      : undefined;
    if (
      range &&
      (!Number.isSafeInteger(range.offset) ||
        range.offset < 0 ||
        (range.length !== undefined &&
          (!Number.isSafeInteger(range.length) || range.length < 1)))
    ) {
      throw new ApiError(
        422,
        "INVALID_RANGE",
        "The requested byte range is invalid.",
      );
    }
    const object = await files.get(
      attachment.object_key,
      range ? { range } : undefined,
    );
    if (!object)
      throw new ApiError(
        503,
        "FILE_UNAVAILABLE",
        "The file is temporarily unavailable.",
      );
    const headers = new Headers({
      "Content-Type": attachment.media_type,
      "Content-Disposition": `${(!!page || attachment.owner_type === "social_draft" || attachment.owner_type.endsWith("_brand")) && attachment.media_type.startsWith("image/") ? "inline" : "attachment"}; filename*=UTF-8''${encodeURIComponent(attachment.original_name)}`,
      "Cache-Control": "private, no-store",
      "X-Content-Type-Options": "nosniff",
      ETag: object.httpEtag,
      "Accept-Ranges": "bytes",
    });
    if (
      range &&
      "range" in object &&
      object.range &&
      "offset" in object.range &&
      "length" in object.range
    ) {
      const offset = object.range.offset;
      const length = object.range.length;
      if (offset !== undefined && length !== undefined) {
        headers.set(
          "Content-Range",
          `bytes ${offset}-${offset + length - 1}/${object.size}`,
        );
        headers.set("Content-Length", String(length));
      }
    }
    return new Response(object.body, { status: range ? 206 : 200, headers });
  } catch (error) {
    return errorResponse(error);
  }
}
