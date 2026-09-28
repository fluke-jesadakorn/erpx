import { z } from "zod";
import { attachmentComponentKinds } from "@/server/attachments/types";
import {
  attachmentContent,
  attachmentLinkStatus,
  completeAttachmentUpload,
  createAttachmentSet,
  listAttachmentSets,
  reserveAttachmentFile,
  withdrawAttachmentLink,
} from "@/server/attachments/service";
import { readBoundedUploadBody } from "@/server/attachments/upload";
import { boundedBody } from "@/server/legal/body";
import {
  customerPortalAttachmentContext,
  customerPortalAttachmentSession,
  customerPortalAttachmentTarget,
  customerPortalSharedPreview,
} from "@/server/customer-portal-attachments";
import { ApiError, errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const noStore = { "Cache-Control": "private, no-store" };
const keyPattern = /^[a-zA-Z0-9:_.-]{8,180}$/;
const createSchema = z.object({
  targetType: z.literal("customer_access_binding"),
  targetId: z.string().min(1).max(180),
  componentKind: z.enum(attachmentComponentKinds).refine((value) => value === "action"),
  componentRef: z.string().max(180).optional(),
  purposeKey: z.literal("customer_support_evidence"),
  expectedTargetVersion: z.number().int().positive(),
}).strict();
const reserveSchema = z.object({
  filename: z.string().min(1).max(255),
  mediaType: z.string().min(1).max(100),
  bytes: z.number().int().positive().max(10 * 1024 * 1024),
  caption: z.string().max(1000).optional(),
}).strict();

function sameOrigin(request: Request) {
  const origin = request.headers.get("origin");
  if (origin && origin !== requestPublicOrigin(request))
    throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
}

function commandKey(request: Request) {
  const key = request.headers.get("Idempotency-Key") ?? "";
  if (!keyPattern.test(key))
    throw new ApiError(422, "COMMAND_KEY_REQUIRED", "A valid Idempotency-Key is required.");
  return key;
}

async function json(request: Request) {
  try {
    const bytes = await boundedBody(request, 32 * 1024);
    return JSON.parse(new TextDecoder().decode(bytes)) as unknown;
  } catch (error) {
    if (error instanceof SyntaxError)
      throw new ApiError(422, "INVALID_JSON", "Invalid JSON.");
    throw error;
  }
}

function previewResponse(result: { object: { body: ReadableStream<Uint8Array> }; mediaType: string }) {
  return new Response(result.object.body, {
    headers: {
      "Content-Type": result.mediaType,
      "Content-Disposition": "inline",
      "Cache-Control": "private, no-store",
      "X-Content-Type-Options": "nosniff",
      "Content-Security-Policy": "sandbox; default-src 'none'",
      "Accept-Ranges": "none",
    },
  });
}

export async function GET(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params;
    if (path[0] === "shared" && path[1] && path[2] === "content") {
      const session = await customerPortalAttachmentSession(holdingSlug);
      return previewResponse(await customerPortalSharedPreview(session.write, session.files, session.actor, path[1]));
    }
    if (path[0] === "attachment-sets" && path.length === 1) {
      const query = new URL(request.url).searchParams;
      const targetType = query.get("targetType");
      const targetId = query.get("targetId") ?? "";
      const purposeKey = query.get("purposeKey");
      const componentKind = query.get("componentKind");
      if (targetType !== "customer_access_binding" || purposeKey !== "customer_support_evidence" || componentKind !== "action")
        throw new ApiError(422, "CUSTOMER_PORTAL_ATTACHMENT_PURPOSE", "Choose the customer support evidence area.");
      const portal = await customerPortalAttachmentContext(holdingSlug, targetId);
      const version = query.get("expectedTargetVersion");
      return Response.json({ data: await listAttachmentSets(portal.write, portal.attachmentActor, {
        targetType, targetId, purposeKey, componentKind: "action",
        expectedTargetVersion: version ? Number(version) : undefined,
      }) }, { headers: noStore });
    }
    if (path[0] === "attachment-links" && path[1] && path[2] === "status") {
      const session = await customerPortalAttachmentSession(holdingSlug);
      const bindingId = await customerPortalAttachmentTarget(session.write, "link", path[1]);
      const portal = await customerPortalAttachmentContext(holdingSlug, bindingId);
      return Response.json({ data: await attachmentLinkStatus(portal.write, portal.attachmentActor, path[1]) }, { headers: noStore });
    }
    if (path[0] === "attachment-links" && path[1] && path[2] === "content") {
      const variant = new URL(request.url).searchParams.get("variant") ?? "preview";
      if (variant !== "preview")
        throw new ApiError(403, "CUSTOMER_PORTAL_PREVIEW_ONLY", "Customer access is limited to sanitized previews.");
      const session = await customerPortalAttachmentSession(holdingSlug);
      const bindingId = await customerPortalAttachmentTarget(session.write, "link", path[1]);
      const portal = await customerPortalAttachmentContext(holdingSlug, bindingId);
      return previewResponse(await attachmentContent(portal.write, portal.files, portal.attachmentActor, path[1], "preview"));
    }
    throw new ApiError(404, "NOT_FOUND", "Customer attachment route not found.");
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, context: Context) {
  try {
    sameOrigin(request);
    const key = commandKey(request);
    const { holdingSlug, path = [] } = await context.params;
    if (path[0] === "attachment-sets" && path.length === 1) {
      const parsed = createSchema.safeParse(await json(request));
      if (!parsed.success)
        throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid customer file set.");
      const portal = await customerPortalAttachmentContext(holdingSlug, parsed.data.targetId);
      return Response.json({ data: await createAttachmentSet(portal.write, portal.attachmentActor, parsed.data, key) }, { status: 201, headers: noStore });
    }
    if (path[0] === "attachment-sets" && path[1] && path[2] === "files") {
      const parsed = reserveSchema.safeParse(await json(request));
      if (!parsed.success)
        throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid customer file reservation.");
      const session = await customerPortalAttachmentSession(holdingSlug);
      const bindingId = await customerPortalAttachmentTarget(session.write, "set", path[1]);
      const portal = await customerPortalAttachmentContext(holdingSlug, bindingId);
      const result = await reserveAttachmentFile(portal.write, portal.attachmentActor, path[1], parsed.data, key);
      return Response.json({ data: {
        ...result,
        uploadUrl: `/api/customer-portal/${encodeURIComponent(holdingSlug)}/attachments/uploads/${encodeURIComponent(result.uploadIntentId)}/content`,
      } }, { status: 201, headers: noStore });
    }
    if (path[0] === "attachment-links" && path[1] && path[2] === "withdraw") {
      const parsed = z.object({ reason: z.string().trim().min(3).max(1000) }).strict().safeParse(await json(request));
      if (!parsed.success)
        throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "A withdrawal reason is required.");
      const session = await customerPortalAttachmentSession(holdingSlug);
      const bindingId = await customerPortalAttachmentTarget(session.write, "link", path[1]);
      const portal = await customerPortalAttachmentContext(holdingSlug, bindingId);
      await withdrawAttachmentLink(portal.write, portal.attachmentActor, path[1], parsed.data.reason, key);
      return Response.json({ data: await attachmentLinkStatus(portal.write, portal.attachmentActor, path[1]) }, { headers: noStore });
    }
    throw new ApiError(404, "NOT_FOUND", "Customer attachment route not found.");
  } catch (error) {
    return errorResponse(error);
  }
}

export async function PUT(request: Request, context: Context) {
  try {
    sameOrigin(request);
    const { holdingSlug, path = [] } = await context.params;
    if (!(path[0] === "uploads" && path[1] && path[2] === "content"))
      throw new ApiError(404, "NOT_FOUND", "Customer upload route not found.");
    const session = await customerPortalAttachmentSession(holdingSlug);
    const bindingId = await customerPortalAttachmentTarget(session.write, "upload", path[1]);
    const portal = await customerPortalAttachmentContext(holdingSlug, bindingId);
    const intent = await portal.write.prepare(`SELECT declared_bytes,media_type FROM upload_intents
      WHERE id=? AND attachment_set_id IS NOT NULL AND created_by=? AND company_id=?`)
      .bind(path[1], portal.attachmentActor.userId, portal.attachmentActor.companyId)
      .first<{ declared_bytes: number; media_type: string }>();
    if (!intent) throw new ApiError(404, "UPLOAD_NOT_FOUND", "Upload intent not found.");
    const contentType = request.headers.get("content-type")?.split(";", 1)[0]?.trim().toLowerCase();
    if (contentType !== intent.media_type)
      throw new ApiError(422, "UPLOAD_TYPE", "Content-Type must match the reserved file type.");
    const buffer = await readBoundedUploadBody(request, Number(intent.declared_bytes));
    return Response.json({ data: await completeAttachmentUpload(
      portal.write, portal.files, portal.attachmentActor, path[1], buffer,
    ) }, { status: 202, headers: noStore });
  } catch (error) {
    return errorResponse(error);
  }
}
