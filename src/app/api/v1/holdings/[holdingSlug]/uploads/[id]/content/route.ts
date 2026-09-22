import {supportEvidenceGuard,supportEvidenceEvent} from "@/server/support-evidence";
import { invoiceEvidenceGuard } from "@/server/invoice-payment-evidence";
import { requestPublicOrigin } from "@/server/public-origin";
import { employeeRequestUpload, requestVersionGuard } from "@/server/employee-requests";
import { serviceAuthoritySnapshot } from "@/server/service-access";
import { socialUploadOwner } from "@/server/social-home";
import { getTask, canAttachTask, taskVersionGuard } from "@/server/staff-tasks";
import { getLeave } from "@/server/leave";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { sha256 } from "@/server/crypto";
import { canEditExpense, getExpense } from "@/server/expenses";
import { assertion, executeCommand, replayCommand } from "@/server/commands";
import { ApiError, errorResponse } from "@/server/http";
import { paymentDraft, paymentGuards } from "@/server/expense-payments";
import { completeBrandUpload } from "@/server/branding";
import { matchesUploadSignature } from "@/server/upload-validation";
import { qualityEvidenceGuard } from "@/server/quality";
import { deliveryEvidenceGuard } from "@/server/commerce-delivery-evidence";

export async function PUT(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/uploads/[id]/content">,
) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const { holdingSlug, id } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const { write, files } = await holdingResources(actor.holdingId);
    const intent = await write
      .prepare(
        `SELECT * FROM upload_intents WHERE id=? AND created_by=? AND (status='ready' OR (status='pending' AND expires_at>?))`,
      )
      .bind(id, actor.userId, Date.now())
      .first<{
        id: string;
        staging_key: string;
        declared_bytes: number;
        media_type: string;
        original_name: string;
        owner_type: string;
        owner_id: string;
        purpose: string;
        revision: number;
        owner_version: number|null;
      }>();
    if (!intent)
      throw new ApiError(
        404,
        "UPLOAD_NOT_FOUND",
        "Upload intent not found or expired.",
      );
    const declaredLength = Number(request.headers.get("content-length"));
    if (
      !Number.isFinite(declaredLength) ||
      declaredLength < 1 ||
      declaredLength > intent.declared_bytes ||
      declaredLength > 10 * 1024 * 1024
    ) {
      throw new ApiError(
        422,
        "UPLOAD_SIZE",
        "The uploaded file size does not match its intent.",
      );
    }
    const buffer = await request.arrayBuffer();
    if (
      buffer.byteLength !== declaredLength ||
      buffer.byteLength > intent.declared_bytes
    )
      throw new ApiError(
        422,
        "UPLOAD_SIZE",
        "The uploaded file size does not match its intent.",
      );
    const bytes = new Uint8Array(buffer);
    if (!matchesUploadSignature(intent.media_type, bytes))
      throw new ApiError(
        422,
        "UPLOAD_TYPE",
        "The file contents do not match the selected type.",
      );
    if (intent.owner_type === "holding_brand" || intent.owner_type === "branch_brand") {
      const result = await completeBrandUpload({
        db: write,
        files,
        actor,
        intent: {
          id,
          owner_type: intent.owner_type,
          owner_id: intent.owner_id,
          purpose: intent.purpose === "inverse" ? "inverse" : "primary",
          revision: intent.revision,
          original_name: intent.original_name,
        },
        buffer,
      });
      return Response.json({ data: result }, { status: 201 });
    }
    const checksum = await sha256(buffer);
    const input = { uploadId: id, checksum, bytes: buffer.byteLength },
      commandId = `upload-${id}`;
    const prior = await replayCommand(
      write,
      actor,
      commandId,
      "attachment.add",
      intent.owner_id,
      input,
    );
    if (prior) return Response.json({ data: prior }, { status: 201 });
    const support=intent.owner_type==="support_ticket"?await supportEvidenceGuard(write,actor,intent.owner_id,Number(intent.owner_version),commandId):null;
    const quality=intent.owner_type==="quality_hold"?await qualityEvidenceGuard(write,actor,intent.owner_id,Number(intent.owner_version),commandId):null;
    const deliveryEvidence=intent.owner_type==="commerce_order_delivery"?await deliveryEvidenceGuard(write,actor,intent.owner_id,Number(intent.owner_version),commandId):null;
    const invoiceEvidence=intent.owner_type==="invoice_payment"?await invoiceEvidenceGuard(write,actor,intent.owner_id,commandId):null;
    const requestSnapshot = intent.owner_type === "employee_request" ? await serviceAuthoritySnapshot(write, actor.companyId) : null;
    const employeeRequest = intent.owner_type === "employee_request" ? await employeeRequestUpload(write, actor, intent.owner_id, intent.purpose) : null;
    const social =
      intent.owner_type === "social_draft"
        ? {
            ...(await socialUploadOwner(write, actor, intent.owner_id, true)),
            department_id: null,
          }
        : null;
    const expense =
      intent.owner_type === "expense_claim"
        ? await getExpense(actor, intent.owner_id)
        : null;
    const payment =
      intent.owner_type === "expense_payment"
        ? await paymentDraft(actor, intent.owner_id, true)
        : null;
    if (expense && !canEditExpense(actor, expense))
      throw new ApiError(
        403,
        "EXPENSE_LOCKED",
        "Receipts require an editable claim.",
      );
    const task =
      intent.owner_type === "staff_task"
        ? await getTask(write, actor, intent.owner_id)
        : null;
    if (task && !canAttachTask(actor, task))
      throw new ApiError(
        403,
        "TASK_LOCKED",
        "Evidence changes require editable work.",
      );
    const leave =
      intent.owner_type === "leave_request"
        ? await getLeave(actor.holdingId, intent.owner_id, actor)
        : null;
    if (
      leave &&
      (leave.created_by !== actor.userId ||
        !["draft", "rejected"].includes(leave.state))
    )
      throw new ApiError(
        403,
        "LEAVE_LOCKED",
        "Evidence changes require your editable request.",
      );
    if (!expense && !task && !leave && !social && !payment && !employeeRequest && !invoiceEvidence && !support && !quality && !deliveryEvidence)
      throw new ApiError(404, "OWNER_NOT_FOUND", "Attachment owner not found.");
    const ownerScope = payment
      ? { ...payment.expense, ...payment.draft }
      : (deliveryEvidence?.row ?? quality?.row ?? support?.row ?? invoiceEvidence?.invoice ?? employeeRequest ?? social ?? expense ?? task ?? leave!);
    const finalKey = `${actor.holdingId}/objects/${id}/r${intent.revision}`;
    const stored = await files.put(finalKey, buffer, {
      onlyIf: { etagDoesNotMatch: "*" },
      httpMetadata: { contentType: intent.media_type },
      customMetadata: { sha256: checksum },
    });
    if (!stored) {
      const existing = await files.head(finalKey);
      if (existing?.customMetadata?.sha256 !== checksum)
        throw new ApiError(
          409,
          "UPLOAD_CONFLICT",
          "This upload already contains different bytes. Create a new upload.",
        );
    }
    const attachmentId = `attachment-${id}`,
      now = Date.now();
    const steps = [
      ...(requestSnapshot ? requestSnapshot(commandId) : []),
      ...(invoiceEvidence?.guards??[]),
      ...(support?.guards??[]),
      ...(quality?.guards??[]),
      ...(deliveryEvidence?.guards??[]),
      ...(payment
        ? paymentGuards(write, actor, commandId, payment.expense)
        : []),
      support ? assertion(write,commandId,"support-purpose","?='support_evidence'",[intent.purpose]) : invoiceEvidence ? assertion(write,commandId,"invoice-slip-type","? IN ('image/jpeg','image/png','image/webp','application/pdf')",[intent.media_type]) : employeeRequest ? requestVersionGuard(write, commandId, employeeRequest) : payment
        ? assertion(
            write,
            commandId,
            "payment-editable",
            "EXISTS(SELECT 1 FROM expense_payment_drafts WHERE id=? AND company_id=? AND version=? AND status='draft')",
            [intent.owner_id, actor.companyId, Number(payment.draft.version)],
          )
        : social
          ? assertion(
              write,
              commandId,
              "social-draft",
              "EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND version=? AND published_post_id IS NULL)",
              [social.id, actor.userId, social.version],
            )
          : deliveryEvidence
            ? assertion(write,commandId,"delivery-purpose","?='proof_of_delivery'",[intent.purpose])
          : task
            ? taskVersionGuard(write, commandId, task.id, task.version, [
                task.state,
              ])
            : expense
              ? assertion(
                  write,
                  commandId,
                  "owner-editable",
                  "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND status IN ('draft','rejected') AND cancelled_at IS NULL)",
                  [intent.owner_id, actor.companyId, Number(expense.version)],
                )
              : assertion(
                  write,
                  commandId,
                  "owner-draft",
                  "EXISTS(SELECT 1 FROM leave_requests WHERE id=? AND company_id=? AND created_by=? AND state IN ('draft','rejected') AND version=?)",
                  [
                    intent.owner_id,
                    actor.companyId,
                    actor.userId,
                    leave!.version,
                  ],
                ),
      assertion(
        write,
        commandId,
        "intent",
        "EXISTS(SELECT 1 FROM upload_intents WHERE id=? AND status='pending' AND expires_at>?)",
        [id, now],
      ),
      write
        .prepare(
          "UPDATE upload_intents SET status='ready',actual_bytes=?,checksum_sha256=?,final_key=?,updated_at=? WHERE id=?",
        )
        .bind(buffer.byteLength, checksum, finalKey, now, id),
      write
        .prepare(
          `INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)`,
        )
        .bind(
          attachmentId,
          id,
          intent.owner_type,
          intent.owner_id,
          intent.purpose,
          finalKey,
          intent.revision,
          intent.original_name,
          intent.media_type,
          buffer.byteLength,
          checksum,
          actor.userId,
          now,
        ),
      write
        .prepare(
          `UPDATE ${deliveryEvidence ? "commerce_orders" : quality ? "inventory_quality_holds" : support ? "support_tickets" : invoiceEvidence ? "finance_invoices" : employeeRequest ? "employee_requests" : payment ? "expense_payment_drafts" : social ? "social_post_drafts" : expense ? "finance_expenses" : task ? "staff_tasks" : "leave_requests"} SET version=version+1,${employeeRequest ? "evidence_version=evidence_version+1," : ""}updated_at=? WHERE id=?`,
        )
        .bind(now, intent.owner_id),
    ];
    if(support)steps.push(supportEvidenceEvent(write,intent.owner_id,commandId,actor.userId,"attachment-add",{attachmentId,checksum,filename:intent.original_name},"Attached private support evidence"));
    const result = await executeCommand(
      write,
      actor,
      commandId,
      "attachment.add",
      input,
      intent.owner_id,
      Number(ownerScope.version) + 1,
      { id: attachmentId, bytes: buffer.byteLength, checksum, ownerVersion:Number(ownerScope.version)+1 },
      steps,
      {
        companyId: String(ownerScope.company_id),
        branchId: ownerScope.branch_id ? String(ownerScope.branch_id) : null,
        departmentId: ownerScope.department_id
          ? String(ownerScope.department_id)
          : null,
      },
    );
    return Response.json({ data: result }, { status: 201 });
  } catch (error) {
    return errorResponse(error);
  }
}
