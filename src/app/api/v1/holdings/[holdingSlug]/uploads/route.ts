import {z} from "zod";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {attachmentsV2ReadOnly} from "@/server/modules/files/public/attachments/feature";
import {attachmentPolicy,authorizeAttachmentTarget} from "@/server/modules/files/public/attachments/policy";
import {createAttachmentSet,reserveAttachmentFile} from "@/server/modules/files/public/attachments/service";
import {legacyAttachmentPolicy} from "@/server/modules/files/public/attachments/legacy-policy";
import {appUploadTaskState} from "@/server/modules/operations/public/app-upload-task-source";
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
        ? await appUploadTaskState(write,parsed.data.ownerId,actor.companyId)
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
    throw new ApiError(503,"ATTACHMENTS_READ_ONLY","Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.");
  } catch (error) {
    return errorResponse(error);
  }
}
