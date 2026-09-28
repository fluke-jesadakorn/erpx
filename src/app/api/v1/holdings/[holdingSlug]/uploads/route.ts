import {supportEvidenceGuard} from "@/server/support-evidence";
import { invoiceEvidenceGuard } from "@/server/invoice-payment-evidence";
import { requestPublicOrigin } from "@/server/public-origin";
import { employeeRequestUpload } from "@/server/employee-requests";
import { serviceAuthoritySnapshot } from "@/server/service-access";
import { socialUploadOwner } from "@/server/social-home";
import { assertion, executeCommand, replayCommand } from "@/server/commands";
import { getTask, canAttachTask } from "@/server/staff-tasks";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { holdingResources } from "@/server/runtime";
import { ApiError, errorResponse } from "@/server/http";
import { canEditExpense, getExpense } from "@/server/expenses";
import { getLeave } from "@/server/leave";
import { paymentDraft, paymentGuards } from "@/server/expense-payments";
import { z } from "zod";
import { requireBrandManagement } from "@/server/branding";
import { qualityEvidenceGuard } from "@/server/quality";
import { deliveryEvidenceGuard } from "@/server/commerce-delivery-evidence";
import { attachmentsV2ReadOnly } from "@/server/attachments/feature";
import { attachmentPolicy, authorizeAttachmentTarget } from "@/server/attachments/policy";
import { createAttachmentSet, reserveAttachmentFile } from "@/server/attachments/service";
import { legacyAttachmentPolicy } from "@/server/attachments/legacy-policy";

const allowedTypes = [
  "image/jpeg",
  "image/png",
  "image/webp",
  "image/heic",
  "image/heif",
  "application/pdf",
  "text/csv",
] as const;
const schema = z.object({
  ownerType: z.enum([
    "support_ticket",
    "employee_request",
    "leave_request",
    "expense_claim",
    "expense_payment",
    "invoice_payment",
    "staff_task",
    "social_draft",
    "holding_brand",
    "branch_brand",
    "company_storefront_brand",
    "quality_hold",
    "commerce_order_delivery",
  ]),
  expectedVersion: z.number().int().positive().optional(),
  ownerId: z.string().min(1).max(128),
  purpose: z.string().min(1).max(64),
  filename: z.string().min(1).max(255),
  mediaType: z.enum(allowedTypes),
  bytes: z
    .number()
    .int()
    .min(1)
    .max(10 * 1024 * 1024),
});

export async function POST(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/uploads">,
) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success)
      throw new ApiError(
        422,
        "VALIDATION_FAILED",
        parsed.error.issues[0]?.message ?? "Invalid upload.",
      );
    if (!attachmentsV2ReadOnly()) {
      const commandKey=request.headers.get("Idempotency-Key")??"";
      if(!/^[a-zA-Z0-9:_.-]{8,180}$/.test(commandKey))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");
      const mapped=legacyAttachmentPolicy(parsed.data.ownerType,parsed.data.purpose);
      if(!mapped)throw new ApiError(422,"ATTACHMENT_PURPOSE_UNSUPPORTED","This legacy file purpose is not registered. Use an explicit attachment set and purpose.");
      const {write}=await holdingResources(actor.holdingId),policy=attachmentPolicy(parsed.data.ownerType,mapped.purposeKey,mapped.componentKind),scope=await authorizeAttachmentTarget(write,actor,policy,parsed.data.ownerId,"write");
      const expectedTargetVersion=parsed.data.expectedVersion??scope.version;
      const dynamicTaskState=mapped.componentRef===null
        ? await write.prepare("SELECT state FROM staff_tasks WHERE id=? AND company_id=?").bind(parsed.data.ownerId,actor.companyId).first<{state:string}>()
        : null;
      const componentRef=mapped.componentRef??(dynamicTaskState?`task:${dynamicTaskState.state}`:null);
      if(!componentRef)throw new ApiError(422,"ATTACHMENT_CONTEXT_UNSUPPORTED","This legacy file owner has no active domain binding context.");
      const set=await createAttachmentSet(write,actor,{targetType:parsed.data.ownerType,targetId:parsed.data.ownerId,componentKind:mapped.componentKind,componentRef,purposeKey:mapped.purposeKey,expectedTargetVersion},`${commandKey}:set`,{compatibilityAdapter:true});
      const reserved=await reserveAttachmentFile(write,actor,set.id,{filename:parsed.data.filename,mediaType:parsed.data.mediaType,bytes:parsed.data.bytes},`${commandKey}:file`,{compatibilityAdapter:true});
      return Response.json({data:{
        id:reserved.uploadIntentId,
        ...reserved,
        attachmentSetId:set.id,
        expectedTargetVersion,
      }},{status:201});
    }
    if(attachmentsV2ReadOnly())throw new ApiError(503,"ATTACHMENTS_READ_ONLY","Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.");
    const requestSnapshot = parsed.data.ownerType === "employee_request" ? await serviceAuthoritySnapshot((await holdingResources(actor.holdingId)).write, actor.companyId) : null;
    if (parsed.data.ownerType === "holding_brand" || parsed.data.ownerType === "branch_brand" || parsed.data.ownerType === "company_storefront_brand") {
      if (!(["image/png", "image/webp"] as string[]).includes(parsed.data.mediaType))
        throw new ApiError(422, "LOGO_TYPE", "Workspace logos accept PNG and WebP images.");
      if (parsed.data.bytes > 2 * 1024 * 1024)
        throw new ApiError(422, "LOGO_SIZE", "Workspace logos must be at most 2 MB.");
      if (parsed.data.ownerType === "company_storefront_brand" ? parsed.data.purpose !== "logo" : !(["primary", "inverse"] as string[]).includes(parsed.data.purpose))
        throw new ApiError(422, "LOGO_PURPOSE", parsed.data.ownerType === "company_storefront_brand" ? "Choose the storefront logo." : "Choose a primary or inverse logo.");
      const { write } = await holdingResources(actor.holdingId);
      await requireBrandManagement(write, actor, parsed.data.ownerType, parsed.data.ownerId);
    } else if (parsed.data.ownerType === "commerce_order_delivery") {
      if(parsed.data.purpose!=="proof_of_delivery")throw new ApiError(422,"DELIVERY_EVIDENCE_PURPOSE","Choose proof of delivery evidence.");
      if(!parsed.data.expectedVersion)throw new ApiError(422,"VERSION_REQUIRED","Refresh the sales order before attaching delivery evidence.");
      await deliveryEvidenceGuard((await holdingResources(actor.holdingId)).write,actor,parsed.data.ownerId,parsed.data.expectedVersion,"delivery-reserve-check");
    } else if (parsed.data.ownerType === "quality_hold") {
      if(parsed.data.purpose!=="quality_inspection_evidence")throw new ApiError(422,"QUALITY_EVIDENCE_PURPOSE","Choose quality inspection evidence.");
      if(!parsed.data.expectedVersion)throw new ApiError(422,"VERSION_REQUIRED","Refresh the quality hold before attaching evidence.");
      await qualityEvidenceGuard((await holdingResources(actor.holdingId)).write,actor,parsed.data.ownerId,parsed.data.expectedVersion,"quality-reserve-check");
    } else if (parsed.data.ownerType === "support_ticket") {
      if(parsed.data.purpose!=="support_evidence")throw new ApiError(422,"SUPPORT_EVIDENCE_PURPOSE","Choose support evidence.");
      await supportEvidenceGuard((await holdingResources(actor.holdingId)).write,actor,parsed.data.ownerId,parsed.data.expectedVersion!,"support-reserve-check");
    } else if (parsed.data.ownerType === "employee_request") {
      await employeeRequestUpload((await holdingResources(actor.holdingId)).write, actor, parsed.data.ownerId, parsed.data.purpose);
    } else if (parsed.data.ownerType === "social_draft") {
      if (!parsed.data.mediaType.startsWith("image/"))
        throw new ApiError(
          422,
          "IMAGE_REQUIRED",
          "Posts accept JPEG, PNG and WebP images.",
        );
      const { write } = await holdingResources(actor.holdingId);
      await socialUploadOwner(write, actor, parsed.data.ownerId, true);
    } else if (parsed.data.ownerType === "expense_claim") {
      const owner = await getExpense(actor, parsed.data.ownerId);
      if (!canEditExpense(actor, owner))
        throw new ApiError(
          403,
          "EXPENSE_LOCKED",
          "Only the claimant or preparer can attach receipts to an editable claim.",
        );
    } else if (parsed.data.ownerType === "invoice_payment") {
      if(parsed.data.mediaType==="text/csv")throw new ApiError(422,"SLIP_DOCUMENT_REQUIRED","Attach the bank slip as a photo or PDF.");
    } else if (parsed.data.ownerType === "expense_payment") {
      if (parsed.data.mediaType === "text/csv")
        throw new ApiError(
          422,
          "SLIP_DOCUMENT_REQUIRED",
          "Attach the bank slip as a photo or PDF.",
        );
      await paymentDraft(actor, parsed.data.ownerId, true);
    } else if (parsed.data.ownerType === "staff_task") {
      const { write } = await holdingResources(actor.holdingId);
      const task = await getTask(write, actor, parsed.data.ownerId);
      if (!canAttachTask(actor, task))
        throw new ApiError(
          403,
          "TASK_LOCKED",
          "Evidence can be changed only in draft or while executing work.",
        );
    } else {
      const owner = await getLeave(actor.holdingId, parsed.data.ownerId, actor);
      if (owner.created_by !== actor.userId)
        throw new ApiError(
          403,
          "FORBIDDEN",
          "Only the request owner can add evidence.",
        );
      if (!["draft", "rejected"].includes(owner.state))
        throw new ApiError(
          409,
          "RECORD_NOT_EDITABLE",
          "Evidence requires a draft request.",
        );
    }
    const { write } = await holdingResources(actor.holdingId);
    const existingKey=request.headers.get("Idempotency-Key");
    if(parsed.data.ownerType==="support_ticket"&&existingKey){const prior=await replayCommand(write,actor,existingKey,"attachment.reserve",parsed.data.ownerId,parsed.data);if(prior)return Response.json({data:prior},{status:201});}
    const current = await write
      .prepare(
        `SELECT count(*) AS count FROM upload_intents
      WHERE owner_type=? AND owner_id=? AND ((status IN ('pending','uploaded') AND expires_at>(floor(extract(epoch FROM clock_timestamp()))::bigint*1000)) OR (status='ready' AND EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=upload_intents.id AND a.revoked_at IS NULL)))`,
      )
      .bind(parsed.data.ownerType, parsed.data.ownerId)
      .first<{ count: number }>();
    if ((current?.count ?? 0) >= 10)
      throw new ApiError(
        422,
        "ATTACHMENT_LIMIT",
        "This item already has ten files.",
      );
    const id = crypto.randomUUID();
    const now = Date.now();
    const stagingKey = `${actor.holdingId}/staging/${id}`;
    const key = request.headers.get("Idempotency-Key") ?? `intent-${id}`;
    const result = {
      id,
      uploadUrl: `/api/v1${workspaceBase(publicScope(actor))}/uploads/${id}/content`,
      expiresAt: now + 15 * 60_000,
    };
    const reserved = await executeCommand(
      write,
      actor,
      key,
      "attachment.reserve",
      parsed.data,
      parsed.data.ownerId,
      1,
      result,
      [
        ...(requestSnapshot ? requestSnapshot(key) : []),
        ...(parsed.data.ownerType==="support_ticket"?(await supportEvidenceGuard(write,actor,parsed.data.ownerId,parsed.data.expectedVersion!,key)).guards:[]),
        ...(parsed.data.ownerType==="quality_hold"?(await qualityEvidenceGuard(write,actor,parsed.data.ownerId,parsed.data.expectedVersion!,key)).guards:[]),
        ...(parsed.data.ownerType==="commerce_order_delivery"?(await deliveryEvidenceGuard(write,actor,parsed.data.ownerId,parsed.data.expectedVersion!,key)).guards:[]),
        ...(parsed.data.ownerType==="invoice_payment"?(await invoiceEvidenceGuard(write,actor,parsed.data.ownerId,key)).guards:[]),
        ...(parsed.data.ownerType === "expense_payment"
          ? paymentGuards(
              write,
              actor,
              key,
              (await paymentDraft(actor, parsed.data.ownerId, true)).expense,
            )
          : []),
        assertion(
          write,
          key,
          "storage-capacity",
          `COALESCE((SELECT SUM(declared_bytes) FROM upload_intents WHERE status NOT IN ('failed','expired')),0)+COALESCE((SELECT SUM(bytes) FROM attachment_derivatives WHERE company_id=?),0)+?<=COALESCE((SELECT storage_bytes FROM beta_policy WHERE company_id=?),9007199254740991)`,
          [actor.companyId, parsed.data.bytes, actor.companyId],
        ),
        assertion(
          write,
          key,
          "file-count",
          `(SELECT count(*) FROM upload_intents WHERE owner_type=? AND owner_id=? AND ((status='ready' AND EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=upload_intents.id AND a.revoked_at IS NULL)) OR (status IN ('pending','uploaded') AND expires_at>?)))<10`,
          [parsed.data.ownerType, parsed.data.ownerId, now],
        ),
        write
          .prepare(
            `INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,
      staging_key,status,created_by,expires_at,created_at,updated_at,owner_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)`,
          )
          .bind(
            id,
            parsed.data.ownerType,
            parsed.data.ownerId,
            parsed.data.purpose,
            parsed.data.filename,
            parsed.data.mediaType,
            parsed.data.bytes,
            stagingKey,
            "pending",
            actor.userId,
            now + 15 * 60_000,
            now,
            now,
            ["support_ticket","quality_hold","commerce_order_delivery"].includes(parsed.data.ownerType)?parsed.data.expectedVersion:null,
          ),
      ],
    );
    return Response.json({ data: reserved }, { status: 201 });
  } catch (error) {
    return errorResponse(error);
  }
}
